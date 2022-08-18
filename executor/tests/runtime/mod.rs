use common::{clean_wasm_project, compile_wasm_project};
use mu::runtime::{Config, Runtime};
use std::{collections::HashMap, path::Path};
use uuid::Uuid;

mod common;
mod providers;

#[tokio::test]
async fn test_simple_func() {
    // Build hello_wasm project
    let hello_wasm_project_path = Path::new("tests/runtime/funcs/hello-wasm");
    let target_dir = compile_wasm_project(&hello_wasm_project_path)
        .await
        .expect("compile wasm project");

    let mut runtime = Runtime::default();

    let id = Uuid::new_v4();
    let path = target_dir.join("hello-wasm.wasm");
    let config = Config::new(id, HashMap::new(), dbg!(path).to_path_buf());
    runtime.load_function(config).await.unwrap();

    let request = r#"{ "req_id": 1, "name": "Chappy" }"#;

    let response = runtime.run_function(id, request.as_bytes()).await.unwrap();
    assert_eq!(
        "{\"req_id\":1,\"result\":\"Hello Chappy, welcome to MuRuntime\"}",
        response
    );

    clean_wasm_project(hello_wasm_project_path).await.unwrap();
}
