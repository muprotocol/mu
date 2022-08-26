use anyhow::{Context, Result};
use mu::{
    mu_stack::{FunctionRuntime, StackID},
    runtime::{
        function::{FunctionDefinition, FunctionID},
        message::gateway::{GatewayRequest, GatewayRequestDetails},
        InvokeFunctionRequest, Request, Runtime,
    },
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::fs;
use utils::{clean_wasm_project, compile_wasm_project};
use uuid::Uuid;

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
        let source = fs::read(&path).await?;

        results.insert(
            id.clone(),
            FunctionDefinition::new(id, source, FunctionRuntime::Wasi1_0, []),
        );
    }

    Ok(results)
}

async fn create_map_function_provider(
    projects: HashMap<&str, &Path>,
) -> Result<MapFunctionProvider> {
    let projects = build_wasm_projects(projects).await?;
    let projects = read_wasm_projects(projects).await?;
    Ok(MapFunctionProvider::new(projects))
}

#[tokio::test]
async fn test_simple_func() {
    let mut projects = HashMap::new();
    projects.insert("hello-wasm", Path::new("tests/runtime/funcs/hello-wasm"));

    let provider = create_map_function_provider(projects.clone())
        .await
        .unwrap();
    let function_ids = provider.ids();
    let runtime = Runtime::start(Box::new(provider));

    let request = GatewayRequestDetails {
        body: "Chappy".into(),
        local_path_and_query: "".into(),
    };
    let message = GatewayRequest::new(1, request);

    let result = runtime
        .mailbox
        .post_and_reply(|r| {
            Request::InvokeFunction(InvokeFunctionRequest {
                message,
                function_id: function_ids[0].clone(),
                reply: r,
            })
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(1, result.id);
    assert_eq!("Hello Chappy, welcome to MuRuntime", result.response.body);
    runtime.shutdown().await.unwrap();
}
