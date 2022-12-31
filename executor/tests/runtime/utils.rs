use anyhow::{bail, Context, Result};
use mu::{
    mudb::{
        self,
        service::{DatabaseID, DatabaseManager},
        DBManagerConfig,
    },
    runtime::{
        start,
        types::{FunctionDefinition, FunctionID, RuntimeConfig},
        Runtime,
    },
    stack::usage_aggregator::UsageAggregator,
};
use mu_stack::FunctionRuntime;
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use tokio::process::Command;

use crate::common::HashMapUsageAggregator;

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
    pub memory_limit: byte_unit::Byte,
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
    build_wasm_projects(projects).await?;
    let functions = read_wasm_functions(projects).await?;
    Ok((functions, MapFunctionProvider::new()))
}

pub async fn create_runtime(
    projects: &[Project],
) -> (Box<dyn Runtime>, DatabaseManager, Box<dyn UsageAggregator>) {
    let config = RuntimeConfig {
        cache_path: PathBuf::from_str("runtime-cache").unwrap(),
    };

    let (functions, provider) = create_map_function_provider(projects).await.unwrap();
    let usage_aggregator = HashMapUsageAggregator::new_boxed();
    let db_manager_config = DBManagerConfig {
        usage_report_duration: Duration::from_secs(1).into(),
    };

    let db_service = DatabaseManager::new(usage_aggregator.clone(), db_manager_config)
        .await
        .unwrap();
    let runtime = start(
        Box::new(provider),
        config,
        db_service.clone(),
        usage_aggregator.clone(),
    )
    .await
    .unwrap();

    runtime
        .add_functions(functions.clone().into_iter().map(|(_, d)| d).collect())
        .await
        .unwrap();

    (runtime, db_service, usage_aggregator)
}
