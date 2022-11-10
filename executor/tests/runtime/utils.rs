use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use mu::{
    mudb::{
        self,
        service::{DatabaseID, DatabaseManager},
    },
    runtime::{
        start,
        types::{FunctionDefinition, FunctionID, RuntimeConfig},
        Runtime,
    },
    stack::usage_aggregator::UsageAggregator,
};
use mu_stack::{FunctionRuntime, MegaByte};
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::process::Command;

use super::providers::MapFunctionProvider;

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
        .wait()
        .await?;
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
        .wait()
        .await?;

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
        .wait()
        .await?;

    Ok(())
}

pub async fn create_db_if_not_exist(
    db_service: DatabaseManager,
    database_id: DatabaseID,
) -> Result<()> {
    let conf = mudb::Config {
        database_id,
        ..Default::default()
    };

    db_service.create_db_if_not_exist(conf).await?;

    Ok(())
}

pub struct Project {
    pub id: FunctionID,
    pub name: String,
    pub path: PathBuf,
    pub memory_limit: MegaByte,
}

impl Project {
    pub fn wasm_module_path(&self) -> PathBuf {
        self.path
            .join("target/wasm32-wasi/release/")
            .join(format!("{}.wasm", self.name))
    }
}

pub async fn build_wasm_projects(projects: &[Project]) -> Result<()> {
    for p in projects {
        compile_wasm_project(&p.path)
            .await
            .context("compile wasm project")?
    }

    Ok(())
}

pub async fn read_wasm_functions(
    projects: &[Project],
) -> Result<HashMap<FunctionID, FunctionDefinition>> {
    let mut results = HashMap::new();

    for project in projects {
        let source = tokio::fs::read(&project.wasm_module_path()).await?.into();

        results.insert(
            project.id.clone(),
            FunctionDefinition::new(
                project.id.clone(),
                source,
                FunctionRuntime::Wasi1_0,
                [],
                project.memory_limit,
            ),
        );
    }

    Ok(results)
}

async fn create_map_function_provider(
    projects: &[Project],
) -> Result<(HashMap<FunctionID, FunctionDefinition>, MapFunctionProvider)> {
    build_wasm_projects(&projects).await?;
    let functions = read_wasm_functions(&projects).await?;
    Ok((functions, MapFunctionProvider::new()))
}

pub async fn create_runtime(projects: &[Project]) -> (Box<dyn Runtime>, DatabaseManager) {
    let config = RuntimeConfig {
        cache_path: PathBuf::from_str("runtime-cache").unwrap(),
    };

    let (functions, provider) = create_map_function_provider(projects).await.unwrap();
    let db_service = DatabaseManager::new().await.unwrap();
    let usage_aggregator = MockUsageAggregator::new();
    let runtime = start(
        Box::new(provider),
        config,
        db_service.clone(),
        usage_aggregator,
    )
    .await
    .unwrap();

    runtime
        .add_functions(functions.clone().into_iter().map(|(_, d)| d).collect())
        .await
        .unwrap();

    (runtime, db_service)
}

//TODO: Need to implement methods and storage so we can test it.
#[derive(Clone)]
pub struct MockUsageAggregator;

impl MockUsageAggregator {
    pub fn new() -> Box<dyn UsageAggregator> {
        Box::new(Self {})
    }
}

#[async_trait]
impl UsageAggregator for MockUsageAggregator {
    fn register_usage(
        &self,
        _stack_id: mu_stack::StackID,
        _usage: Vec<mu::stack::usage_aggregator::Usage>,
    ) {
    }

    async fn get_and_reset_usages(
        &self,
    ) -> Result<HashMap<mu_stack::StackID, HashMap<mu::stack::usage_aggregator::UsageCategory, u128>>>
    {
        Ok(HashMap::new())
    }

    async fn stop(&self) {}
}
