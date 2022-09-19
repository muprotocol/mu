use anyhow::{Context, Result};
use futures::FutureExt;
use mu::{
    gateway,
    mu_stack::{self, FunctionRuntime, StackID},
    mudb::service::{DatabaseID, Service as DbService},
    runtime::{start, types::*, Runtime},
};
use serial_test::serial;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::fs;
use utils::{clean_wasm_project, compile_wasm_project};
use uuid::Uuid;

use crate::runtime::utils::create_db_if_not_exist;

use self::providers::MapFunctionProvider;

mod providers;
mod utils;

//TODO: maybe some `make clean` usage for this function
#[allow(dead_code)]
async fn clean_wasm_projects(projects: HashMap<&str, &Path>) -> Result<()> {
    for (_, path) in projects {
        clean_wasm_project(path).await?;
    }
    Ok(())
}

async fn build_wasm_projects(projects: HashMap<&str, &Path>) -> Result<HashMap<String, PathBuf>> {
    let mut results = HashMap::new();

    for (name, path) in projects {
        let wasm_file = compile_wasm_project(path)
            .await
            .context("compile wasm project")?
            .join(format!("{name}.wasm"));
        results.insert(name.into(), wasm_file);
    }

    Ok(results)
}

async fn read_wasm_projects(
    projects: HashMap<String, PathBuf>,
) -> Result<HashMap<FunctionID, FunctionDefinition>> {
    let mut results = HashMap::new();

    for (_, path) in projects {
        let id = FunctionID {
            stack_id: StackID(Uuid::new_v4()),
            function_name: "my_func".into(),
        };
        let source = fs::read(&path).await?.into();

        results.insert(
            id.clone(),
            FunctionDefinition::new(id, source, FunctionRuntime::Wasi1_0, []),
        );
    }

    Ok(results)
}

async fn create_map_function_provider(
    projects: HashMap<&str, &Path>,
) -> Result<(HashMap<FunctionID, FunctionDefinition>, MapFunctionProvider)> {
    let projects = build_wasm_projects(projects).await?;
    let projects = read_wasm_projects(projects).await?;
    Ok((projects, MapFunctionProvider::new()))
}

async fn create_runtime(
    projects: HashMap<&str, &Path>,
) -> (Box<dyn Runtime>, Vec<FunctionID>, DbService) {
    let config = RuntimeConfig {
        cache_path: PathBuf::from_str("runtime-cache").unwrap(),
    };

    let (projects, provider) = create_map_function_provider(projects).await.unwrap();
    let db_service = DbService::new().await.unwrap();
    let mut runtime = start(Box::new(provider), config, db_service.clone())
        .await
        .unwrap();

    let functions: Vec<FunctionDefinition> = projects.into_values().collect();
    let function_ids = functions
        .clone()
        .into_iter()
        .map(|f| f.id.clone())
        .collect();

    runtime.add_functions(functions).await.unwrap();

    (runtime, function_ids, db_service)
}

#[tokio::test]
#[serial]
async fn test_simple_func() {
    let mut projects = HashMap::new();
    projects.insert("hello-wasm", Path::new("tests/runtime/funcs/hello-wasm"));

    let (runtime, function_ids, _) = create_runtime(projects.clone()).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Chappy",
    };

    let (resp, _usage) = runtime
        .invoke_function(function_ids[0].clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Chappy, welcome to MuRuntime", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_query_mudb() {
    let mut projects = HashMap::new();
    projects.insert("hello-mudb", Path::new("tests/runtime/funcs/hello-mudb"));

    let (runtime, function_ids, db_service) = create_runtime(projects.clone()).await;

    let database_id = DatabaseID {
        stack_id: function_ids[0].stack_id.clone(),
        db_name: "my_db".into(),
    };

    create_db_if_not_exist(db_service, database_id)
        .await
        .unwrap();

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Dream",
    };

    let (resp, _usage) = runtime
        .invoke_function(function_ids[0].clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Dream", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_multiple_instance_of_the_same_function() {
    let mut projects = HashMap::new();
    projects.insert("hello-wasm", Path::new("tests/runtime/funcs/hello-wasm"));

    let (runtime, function_ids, _) = create_runtime(projects.clone()).await;

    let make_request = |name| gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: name,
    };

    let instance_1 = runtime
        .invoke_function(function_ids[0].clone(), make_request("Mathew"))
        .then(
            |r| async move { assert_eq!("Hello Mathew, welcome to MuRuntime", r.unwrap().0.body) },
        );
    println!("First instance created");

    let instance_2 =
        runtime
            .invoke_function(function_ids[0].clone(), make_request("Morphius"))
            .then(|r| async move {
                assert_eq!("Hello Morphius, welcome to MuRuntime", r.unwrap().0.body)
            });
    println!("Second instance created");

    let instance_3 = runtime
        .invoke_function(function_ids[0].clone(), make_request("Unity"))
        .then(
            |r| async move { assert_eq!("Hello Unity, welcome to MuRuntime", r.unwrap().0.body) },
        );
    println!("Third instance created");

    tokio::join!(instance_1, instance_2, instance_3);
    println!("All instance joined");

    runtime.shutdown().await.unwrap();
}
