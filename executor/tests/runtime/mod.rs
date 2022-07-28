use mu::runtime::{Config, MuRuntime};
use std::{collections::HashMap, path::Path};
use uuid::Uuid;

#[tokio::test]
async fn test_simple_func() {
    let mut runtime = MuRuntime::new();

    let id = Uuid::new_v4();
    let path = Path::new("tests/runtime/funcs/hello-wasm/module.wasm");
    let config = Config::new(id, HashMap::new(), path.to_path_buf());
    runtime.load_function(config).await.unwrap();

    let request = r#"{ "req_id": 1, "name": "Chappy" }"#;

    let response = runtime.run_function(id, request.as_bytes()).await.unwrap();
    assert_eq!(
        "{\"req_id\":1,\"result\":\"Hello Chappy, welcome to MuRuntime\"}",
        response
    );
}
