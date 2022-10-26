use anyhow::{Context, Result};
use futures::FutureExt;
use mu::{
    gateway,
    mudb::service::{DatabaseID, DatabaseManager},
    runtime::{start, types::*, Runtime},
};
use mu_stack::{self, FunctionRuntime, StackID};
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

struct Project {
    pub name: String,
    pub stack_id: StackID,
    pub path: PathBuf,
}

impl Project {
    pub fn wasm_module_path(&self) -> PathBuf {
        self.path
            .join("target/wasm32-wasi/release/")
            .join(format!("{}.wasm", self.name))
    }
}

async fn build_wasm_projects(projects: &[Project]) -> Result<()> {
    for p in projects {
        compile_wasm_project(&p.path)
            .await
            .context("compile wasm project")?
    }

    Ok(())
}

async fn read_wasm_functions(
    projects: &[Project],
) -> Result<HashMap<FunctionID, FunctionDefinition>> {
    let mut results = HashMap::new();

    for project in projects {
        let id = FunctionID {
            stack_id: project.stack_id,
            function_name: "my_func".into(),
        };
        let source = fs::read(&project.wasm_module_path()).await?.into();

        results.insert(
            id.clone(),
            FunctionDefinition::new(id, source, FunctionRuntime::Wasi1_0, []),
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

async fn create_runtime(
    projects: &[Project],
) -> (Box<dyn Runtime>, Vec<FunctionDefinition>, DatabaseManager) {
    let config = RuntimeConfig {
        cache_path: PathBuf::from_str("runtime-cache").unwrap(),
    };

    let (functions, provider) = create_map_function_provider(projects).await.unwrap();
    let db_service = DatabaseManager::new().await.unwrap();
    let runtime = start(Box::new(provider), config, db_service.clone())
        .await
        .unwrap();

    let functions: Vec<FunctionDefinition> = functions.into_values().collect();

    runtime.add_functions(functions.clone()).await.unwrap();

    (runtime, functions, db_service)
}

#[tokio::test]
#[serial]
async fn test_simple_func() {
    let projects = vec![Project {
        name: "hello-wasm".into(),
        path: Path::new("tests/runtime/funcs/hello-wasm").into(),
        stack_id: StackID(Uuid::new_v4()),
    }];

    let (runtime, functions, _) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Chappy",
    };

    let (resp, _usage) = runtime
        .invoke_function(functions[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Chappy, welcome to MuRuntime", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_query_mudb() {
    let projects = vec![Project {
        name: "hello-mudb".into(),
        path: Path::new("tests/runtime/funcs/hello-mudb").into(),
        stack_id: StackID(Uuid::new_v4()),
    }];

    let (runtime, functions, db_service) = create_runtime(&projects).await;

    let database_id = DatabaseID {
        stack_id: functions[0].id.stack_id,
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
        .invoke_function(functions[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Dream", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_multiple_instance_of_the_same_function() {
    let projects = vec![Project {
        name: "hello-wasm".into(),
        path: Path::new("tests/runtime/funcs/hello-wasm").into(),
        stack_id: StackID(Uuid::new_v4()),
    }];

    let (runtime, functions, _) = create_runtime(&projects).await;

    let make_request = |name| gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: name,
    };

    let instance_1 = runtime
        .invoke_function(functions[0].id.clone(), make_request("Mathew"))
        .then(
            |r| async move { assert_eq!("Hello Mathew, welcome to MuRuntime", r.unwrap().0.body) },
        );

    let instance_2 =
        runtime
            .invoke_function(functions[0].id.clone(), make_request("Morphius"))
            .then(|r| async move {
                assert_eq!("Hello Morphius, welcome to MuRuntime", r.unwrap().0.body)
            });

    let instance_3 = runtime
        .invoke_function(functions[0].id.clone(), make_request("Unity"))
        .then(
            |r| async move { assert_eq!("Hello Unity, welcome to MuRuntime", r.unwrap().0.body) },
        );

    tokio::join!(instance_1, instance_2, instance_3);

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_instances_of_different_functions() {
    let projects = vec![
        Project {
            name: "hello-wasm".into(),
            path: Path::new("tests/runtime/funcs/hello-wasm").into(),
            stack_id: StackID(Uuid::new_v4()),
        },
        Project {
            name: "hello-mudb".into(),
            path: Path::new("tests/runtime/funcs/hello-mudb").into(),
            stack_id: StackID(Uuid::new_v4()),
        },
    ];

    let (runtime, functions, db_service) = create_runtime(&projects).await;

    let make_request = |name| gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: name,
    };

    let database_id = DatabaseID {
        stack_id: projects[1].stack_id,
        db_name: "my_db".into(),
    };

    create_db_if_not_exist(db_service, database_id)
        .await
        .unwrap();

    let instance_1 = runtime
        .invoke_function(functions[0].id.clone(), make_request("Mathew"))
        .then(
            |r| async move { assert_eq!("Hello Mathew, welcome to MuRuntime", r.unwrap().0.body) },
        );

    let instance_2 = runtime
        .invoke_function(functions[1].id.clone(), make_request("Dream"))
        .then(|r| async move { assert_eq!("Hello Dream", r.unwrap().0.body) });

    tokio::join!(instance_1, instance_2);

    runtime.shutdown().await.unwrap();
}
