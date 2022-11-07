use futures::FutureExt;
use mu::{gateway, mudb::service::DatabaseID, runtime::types::FunctionID};
use mu_stack::{self, StackID};
use serial_test::serial;
use std::{collections::HashMap, path::Path};

use crate::runtime::utils::{create_db_if_not_exist, create_runtime, Project};

mod providers;
mod utils;

pub fn create_project(name: &'static str) -> Project {
    Project {
        name: name.into(),
        path: Path::new(&format!("tests/runtime/funcs/{name}")).into(),
        id: FunctionID {
            stack_id: StackID::SolanaPublicKey(rand::random()),
            function_name: name.into(),
        },
    }
}

#[tokio::test]
#[serial]
async fn test_simple_func() {
    let projects = vec![create_project("hello-wasm")];
    let (runtime, ..) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Chappy",
    };

    let (resp, _usage) = runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Chappy, welcome to MuRuntime", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_query_mudb() {
    let projects = vec![create_project("hello-mudb")];
    let (runtime, db_service) = create_runtime(&projects).await;

    let database_id = DatabaseID {
        stack_id: projects[0].id.stack_id,
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
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Dream", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_multiple_instance_of_the_same_function() {
    let projects = vec![create_project("hello-wasm")];
    let (runtime, _) = create_runtime(&projects).await;

    let make_request = |name| gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: name,
    };

    let instance_1 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Mathew"))
        .then(
            |r| async move { assert_eq!("Hello Mathew, welcome to MuRuntime", r.unwrap().0.body) },
        );

    let instance_2 =
        runtime
            .invoke_function(projects[0].id.clone(), make_request("Morpheus"))
            .then(|r| async move {
                assert_eq!("Hello Morpheus, welcome to MuRuntime", r.unwrap().0.body)
            });

    let instance_3 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Unity"))
        .then(
            |r| async move { assert_eq!("Hello Unity, welcome to MuRuntime", r.unwrap().0.body) },
        );

    tokio::join!(instance_1, instance_2, instance_3);

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_instances_of_different_functions() {
    let projects = vec![create_project("hello-wasm"), create_project("hello-mudb")];
    let (runtime, db_service) = create_runtime(&projects).await;

    let make_request = |name| gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: name,
    };

    let database_id = DatabaseID {
        stack_id: projects[1].id.stack_id,
        db_name: "my_db".into(),
    };

    create_db_if_not_exist(db_service, database_id)
        .await
        .unwrap();

    let instance_1 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Mathew"))
        .then(
            |r| async move { assert_eq!("Hello Mathew, welcome to MuRuntime", r.unwrap().0.body) },
        );

    let instance_2 = runtime
        .invoke_function(projects[1].id.clone(), make_request("Dream"))
        .then(|r| async move { assert_eq!("Hello Dream", r.unwrap().0.body) });

    tokio::join!(instance_1, instance_2);

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_functions_with_early_exit_are_handled() {
    let projects = vec![create_project("early-exit")];
    let (runtime, _) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Are You There?",
    };

    match runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
    {
        Err(e) => assert!(e.to_string().contains("Function exited early")),
        _ => panic!("Early exit function should fail to run"),
    }

    runtime.shutdown().await.unwrap();
}
