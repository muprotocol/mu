use anyhow::{Context, Result};
use mu::runtime::{
    function::{Config, FunctionDefinition, FunctionID},
    message::gateway::{GatewayRequest, GatewayResponse},
    types::ID,
    Request, Runtime,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::fs;
use utils::{clean_wasm_project, compile_wasm_project};

use self::providers::MapFunctionProvider;

mod providers;
mod utils;

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
        let id = ID::gen();
        let source = fs::read(&path).await?;
        let config = Config::new(id, HashMap::new(), path);

        results.insert(id, FunctionDefinition::new(source, config));
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
    let runtime = Runtime::new(provider).start();

    let request = r#"{ "req_id": 1, "name": "Chappy" }"#.to_owned();
    let message = GatewayRequest::new(1, function_ids[0], request);

    let response: Result<GatewayResponse> = runtime
        .post_and_reply(|r| Request::Gateway { message, reply: r })
        .await
        .unwrap();

    assert_eq!(
        "{\"req_id\":1,\"result\":\"Hello Chappy, welcome to MuRuntime\"}",
        response.unwrap().response
    );

    //clean_wasm_projects(projects).await.unwrap();
}
