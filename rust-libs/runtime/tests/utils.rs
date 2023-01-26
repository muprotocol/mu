use std::{
    collections::{hash_map::Entry, HashMap},
    env,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use tokio::sync::Mutex;

use async_trait::async_trait;
use mu_runtime::{
    start, AssemblyDefinition, AssemblyProvider, Notification, Runtime, RuntimeConfig, Usage,
};
use mu_stack::{AssemblyID, AssemblyRuntime, FunctionID, StackID};

#[derive(Default)]
pub struct MapAssemblyProvider {
    inner: HashMap<AssemblyID, AssemblyDefinition>,
}

#[async_trait]
impl AssemblyProvider for MapAssemblyProvider {
    fn get(&self, id: &AssemblyID) -> Option<&AssemblyDefinition> {
        Some(self.inner.get(id).unwrap())
    }

    fn add_function(&mut self, function: AssemblyDefinition) {
        self.inner.insert(function.id.clone(), function);
    }

    fn remove_function(&mut self, id: &AssemblyID) {
        self.inner.remove(id);
    }

    fn get_function_names(&self, _stack_id: &StackID) -> Vec<String> {
        unimplemented!("Not needed")
    }
}

fn ensure_project_dir(project_dir: &Path) -> Result<PathBuf> {
    let project_dir = env::current_dir()?.join(project_dir);
    if !project_dir.is_dir() {
        bail!("project dir should be a directory")
    }
    Ok(project_dir)
}

async fn install_wasm32_wasi_target() -> Result<()> {
    Command::new("rustup")
        .args(["target", "add", "wasm32-wasi"])
        .spawn()?
        .wait()?;
    Ok(())
}

pub async fn compile_wasm_project(project_dir: &Path) -> Result<()> {
    let project_dir = ensure_project_dir(project_dir)?;
    install_wasm32_wasi_target().await?;

    Command::new("cargo")
        .current_dir(&project_dir)
        .env_remove("CARGO_TARGET_DIR")
        .arg("build")
        .args(["--release", "--target", "wasm32-wasi"])
        .spawn()?
        .wait()?;

    Ok(())
}

//TODO: maybe some `make clean` usage for this function
#[allow(dead_code)]
pub async fn clean_wasm_project(project_dir: &Path) -> Result<()> {
    let project_dir = ensure_project_dir(project_dir)?;

    Command::new("cargo")
        .current_dir(&project_dir)
        .env_remove("CARGO_TARGET_DIR")
        .arg("clean")
        .spawn()?
        .wait()?;

    Ok(())
}

// pub async fn create_db_if_not_exist(
//     db_service: DatabaseManager,
//     database_id: DatabaseID,
// ) -> Result<()> {
//     let conf = mudb::Config {
//         database_id,
//         ..Default::default()
//     };

//     db_service.create_db_if_not_exist(conf).await?;

//     Ok(())
// }

pub struct Project<'a> {
    pub id: AssemblyID,
    pub name: &'a str,
    pub path: PathBuf,
    pub memory_limit: byte_unit::Byte,
    pub functions: &'a [&'a str],
}

impl<'a> Project<'a> {
    pub fn wasm_module_path(&self) -> PathBuf {
        self.path
            .join("target/wasm32-wasi/release/")
            .join(format!("{}.wasm", self.name))
    }

    pub fn function_id(&self, index: usize) -> Option<FunctionID> {
        if index <= self.functions.len() {
            Some(FunctionID {
                assembly_id: self.id.clone(),
                function_name: self.functions[index].to_string(),
            })
        } else {
            None
        }
    }
}

pub async fn build_wasm_projects<'a>(projects: &'a [Project<'a>]) -> Result<()> {
    for p in projects {
        compile_wasm_project(&p.path)
            .await
            .context("compile wasm project")?
    }

    Ok(())
}

pub async fn read_wasm_functions<'a>(
    projects: &'a [Project<'a>],
) -> Result<HashMap<AssemblyID, AssemblyDefinition>> {
    let mut results = HashMap::new();

    for project in projects {
        let source = std::fs::read(project.wasm_module_path())?;

        results.insert(
            project.id.clone(),
            AssemblyDefinition::new(
                project.id.clone(),
                source.into(),
                AssemblyRuntime::Wasi1_0,
                [],
                project.memory_limit,
            ),
        );
    }

    Ok(results)
}

async fn create_map_function_provider<'a>(
    projects: &'a [Project<'a>],
) -> Result<(HashMap<AssemblyID, AssemblyDefinition>, MapAssemblyProvider)> {
    build_wasm_projects(projects).await?;
    let functions = read_wasm_functions(projects).await?;
    Ok((functions, MapAssemblyProvider::default()))
}

pub async fn create_runtime<'a>(
    projects: &'a [Project<'a>],
) -> (Box<dyn Runtime>, Arc<Mutex<HashMap<StackID, Usage>>>) {
    let config = RuntimeConfig {
        cache_path: PathBuf::from_str("runtime-cache").unwrap(),
        include_function_logs: true,
    };

    let (functions, provider) = create_map_function_provider(projects).await.unwrap();
    //let db_manager_config = DBManagerConfig {
    //    usage_report_duration: Duration::from_secs(1).into(),
    //};

    //let db_service = DatabaseManager::new(usage_aggregator.clone(), db_manager_config)
    //    .await
    //    .unwrap();
    let (runtime, mut notifications) = start(Box::new(provider), config).await.unwrap();

    runtime
        .add_functions(functions.clone().into_values().into_iter().collect())
        .await
        .unwrap();

    let usages = Arc::new(Mutex::new(HashMap::new()));

    tokio::spawn({
        let usages = usages.clone();
        async move {
            loop {
                match notifications.recv().await {
                    None => continue,
                    Some(n) => match n {
                        Notification::ReportUsage(stack_id, usage) => {
                            let mut map = usages.lock().await;
                            if let Entry::Vacant(e) = map.entry(stack_id) {
                                e.insert(usage);
                            } else {
                                *map.get_mut(&stack_id).unwrap() += usage;
                            }
                        }
                    },
                }
            }
        }
    });

    (runtime, usages)
}
