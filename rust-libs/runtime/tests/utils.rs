use std::{
    collections::{hash_map::Entry, HashMap},
    env, fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use mu_db::{
    DbManager, DbManagerImpl, IpAndPort, NodeAddress, PdConfig, TikvConfig, TikvRunnerConfig,
};
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

    fn remove_all_functions(&mut self, _stack_id: &StackID) -> Option<Vec<String>> {
        unimplemented!("Not needed for tests")
    }

    fn get_function_names(&self, _stack_id: &StackID) -> Vec<String> {
        unimplemented!("Not needed")
    }
}

const TEST_DATA_DIR: &str = "tests/runtime/test_data";

fn clean_data_dir() {
    match fs::remove_dir_all(TEST_DATA_DIR) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => Err(e).unwrap(),
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

fn make_node_address(port: u16) -> NodeAddress {
    NodeAddress {
        address: "127.0.0.1".parse().unwrap(),
        port,
    }
}
fn make_tikv_runner_conf(peer_port: u16, client_port: u16, tikv_port: u16) -> TikvRunnerConfig {
    let localhost: IpAddr = "127.0.0.1".parse().unwrap();
    TikvRunnerConfig {
        pd: PdConfig {
            peer_url: IpAndPort {
                address: localhost,
                port: peer_port,
            },
            client_url: IpAndPort {
                address: localhost,
                port: client_port,
            },
            data_dir: format!("{TEST_DATA_DIR}/pd_data_dir_{peer_port}"),
            log_file: Some(format!("{TEST_DATA_DIR}/pd_log_{peer_port}")),
        },
        node: TikvConfig {
            cluster_url: IpAndPort {
                address: localhost,
                port: tikv_port,
            },
            data_dir: format!("{TEST_DATA_DIR}/tikv_data_dir_{tikv_port}"),
            log_file: Some(format!("{TEST_DATA_DIR}/tikv_log_{tikv_port}")),
        },
    }
}

pub async fn create_runtime<'a>(
    projects: &'a [Project<'a>],
) -> (
    Box<dyn Runtime>,
    Box<dyn DbManager>,
    Arc<Mutex<HashMap<StackID, Usage>>>,
) {
    clean_data_dir();
    let config = RuntimeConfig {
        cache_path: PathBuf::from_str("runtime-cache").unwrap(),
        include_function_logs: true,
    };

    let (functions, provider) = create_map_function_provider(projects).await.unwrap();
    let node_address = make_node_address(2803);
    let tikv_config = make_tikv_runner_conf(2385, 2386, 20163);

    let db_manager = DbManagerImpl::new_with_embedded_cluster(node_address, vec![], tikv_config)
        .await
        .unwrap();

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

    (runtime, Box::new(db_manager), usages)
}
