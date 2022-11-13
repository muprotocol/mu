use futures::FutureExt;
use mu::{
    gateway, mudb::service::DatabaseID, runtime::types::FunctionID,
    stack::usage_aggregator::UsageCategory,
};
use mu_stack::{self, MegaByte, StackID};
use serial_test::serial;
use std::{collections::HashMap, path::Path};

use crate::runtime::utils::{create_db_if_not_exist, create_runtime, Project};

mod providers;
mod utils;

pub fn create_project(name: &'static str, memory_limit: Option<MegaByte>) -> Project {
    Project {
        name: name.into(),
        path: Path::new(&format!("tests/runtime/funcs/{name}")).into(),
        id: FunctionID {
            stack_id: StackID::SolanaPublicKey(rand::random()),
            function_name: name.into(),
        },
        memory_limit: memory_limit.unwrap_or(MegaByte(100)),
    }
}

#[tokio::test]
#[serial]
async fn test_simple_func() {
    let projects = vec![create_project("hello-wasm", None)];
    let (runtime, _, _) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Chappy",
    };

    let resp = runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Chappy, welcome to MuRuntime", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_query_mudb() {
    let projects = vec![create_project("hello-mudb", None)];
    let (runtime, db_service, _) = create_runtime(&projects).await;

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

    let resp = runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Dream", resp.body);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_multiple_instance_of_the_same_function() {
    let projects = vec![create_project("hello-wasm", None)];
    let (runtime, _, _) = create_runtime(&projects).await;

    let make_request = |name| gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: name,
    };

    let instance_1 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Mathew"))
        .then(|r| async move { assert_eq!("Hello Mathew, welcome to MuRuntime", r.unwrap().body) });

    let instance_2 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Morpheus"))
        .then(
            |r| async move { assert_eq!("Hello Morpheus, welcome to MuRuntime", r.unwrap().body) },
        );

    let instance_3 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Unity"))
        .then(|r| async move { assert_eq!("Hello Unity, welcome to MuRuntime", r.unwrap().body) });

    tokio::join!(instance_1, instance_2, instance_3);

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_instances_of_different_functions() {
    let projects = vec![
        create_project("hello-wasm", None),
        create_project("hello-mudb", None),
    ];
    let (runtime, db_service, ..) = create_runtime(&projects).await;

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
        .then(|r| async move { assert_eq!("Hello Mathew, welcome to MuRuntime", r.unwrap().body) });

    let instance_2 = runtime
        .invoke_function(projects[1].id.clone(), make_request("Dream"))
        .then(|r| async move { assert_eq!("Hello Dream", r.unwrap().body) });

    tokio::join!(instance_1, instance_2);

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_functions_with_early_exit_are_handled() {
    let projects = vec![create_project("early-exit", None)];
    let (runtime, _, _) = create_runtime(&projects).await;

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

#[tokio::test]
#[serial]
async fn functions_with_limited_memory_wont_run() {
    let projects = vec![create_project("memory-heavy", Some(MegaByte(1)))];
    let (runtime, ..) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "",
    };

    let result = runtime
        .invoke_function(projects[0].id.clone(), request)
        .await;

    assert_eq!(
        result
            .err()
            .unwrap()
            .downcast::<mu::runtime::error::Error>()
            .unwrap(),
        mu::runtime::error::Error::MaximumMemoryExceeded
    );

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn functions_with_limited_memory_will_run_with_enough_memory() {
    let projects = vec![create_project("memory-heavy", Some(MegaByte(120)))];
    let (runtime, ..) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Test",
    };

    runtime
        .invoke_function(projects[0].id.clone(), request)
        .then(|r| async move { assert_eq!("Hello Test, i ran!", r.unwrap().body) })
        .await;

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn function_usage_is_reported_correctly_1() {
    let projects = vec![create_project("hello-wasm", None)];
    let (runtime, _, usage_aggregator) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Chappy",
    };

    runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    let usages = usage_aggregator.get_and_reset_usages().await.unwrap();
    let function_usage = usages.get(&projects[0].id.stack_id).unwrap();

    assert_eq!(
        function_usage.get(&UsageCategory::DBWrites),
        Some(0u128).as_ref()
    );

    assert_eq!(
        function_usage.get(&UsageCategory::DBReads),
        Some(0u128).as_ref()
    );

    assert!(
        function_usage
            .get(&UsageCategory::FunctionMBInstructions)
            .unwrap()
            > &0
    );

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn function_usage_is_reported_correctly_2() {
    let projects = vec![create_project("hello-mudb", None)];
    let (runtime, db_service, usage_aggregator) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: "/get_name",
        query: HashMap::new(),
        headers: Vec::new(),
        data: "Chappy",
    };

    let database_id = DatabaseID {
        stack_id: projects[0].id.stack_id,
        db_name: "my_db".into(),
    };

    create_db_if_not_exist(db_service, database_id)
        .await
        .unwrap();

    runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    let usages = usage_aggregator.get_and_reset_usages().await.unwrap();
    let function_usage = usages.get(&projects[0].id.stack_id).unwrap();

    assert!(function_usage.get(&UsageCategory::DBWrites).unwrap() > &0);

    assert!(function_usage.get(&UsageCategory::DBReads).unwrap() > &0);

    assert!(
        function_usage
            .get(&UsageCategory::FunctionMBInstructions)
            .unwrap()
            > &0
    );

    runtime.shutdown().await.unwrap();
}
