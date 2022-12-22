use futures::FutureExt;
use mu::{
    gateway, mudb::service::DatabaseID, runtime::types::FunctionID,
    stack::usage_aggregator::UsageCategory,
};
use mu_stack::{self, StackID};
use serial_test::serial;
use std::{borrow::Cow, collections::HashMap, path::Path};

use crate::runtime::utils::{create_db_if_not_exist, create_runtime, Project};

mod providers;
mod utils;

pub fn create_project(name: &'static str, memory_limit: Option<byte_unit::Byte>) -> Project {
    let memory_limit = memory_limit
        .unwrap_or_else(|| byte_unit::Byte::from_unit(100.0, byte_unit::ByteUnit::MB).unwrap());

    Project {
        name: name.into(),
        path: Path::new(&format!("tests/runtime/funcs/{name}")).into(),
        id: FunctionID {
            stack_id: StackID::SolanaPublicKey(rand::random()),
            function_name: name.into(),
        },
        memory_limit,
    }
}

#[tokio::test]
#[serial]
async fn test_simple_func() {
    let projects = vec![create_project("hello-wasm", None)];
    let (runtime, _, _) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed("Chappy".as_bytes()),
    };

    let resp = runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!("Hello Chappy, welcome to MuRuntime".as_bytes(), resp.body);
    runtime.stop().await.unwrap();
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
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed("Dream".as_bytes()),
    };

    let resp = runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
        .unwrap();

    assert_eq!(Cow::Borrowed("Hello Dream".as_bytes()), resp.body);
    runtime.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_multiple_instance_of_the_same_function() {
    let projects = vec![create_project("hello-wasm", None)];
    let (runtime, _, _) = create_runtime(&projects).await;

    let make_request = |name| gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed(name),
    };

    let instance_1 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Mathew".as_bytes()))
        .then(|r| async move {
            assert_eq!(
                "Hello Mathew, welcome to MuRuntime".as_bytes(),
                r.unwrap().body
            )
        });

    let instance_2 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Morpheus".as_bytes()))
        .then(|r| async move {
            assert_eq!(
                "Hello Morpheus, welcome to MuRuntime".as_bytes(),
                r.unwrap().body
            )
        });

    let instance_3 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Unity".as_bytes()))
        .then(|r| async move {
            assert_eq!(
                "Hello Unity, welcome to MuRuntime".as_bytes(),
                r.unwrap().body
            )
        });

    tokio::join!(instance_1, instance_2, instance_3);

    runtime.stop().await.unwrap();
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
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed(name),
    };

    let database_id = DatabaseID {
        stack_id: projects[1].id.stack_id,
        db_name: "my_db".into(),
    };

    create_db_if_not_exist(db_service, database_id)
        .await
        .unwrap();

    let instance_1 = runtime
        .invoke_function(projects[0].id.clone(), make_request("Mathew".as_bytes()))
        .then(|r| async move {
            assert_eq!(
                "Hello Mathew, welcome to MuRuntime".as_bytes(),
                r.unwrap().body
            )
        });

    let instance_2 = runtime
        .invoke_function(projects[1].id.clone(), make_request("Dream".as_bytes()))
        .then(|r| async move { assert_eq!("Hello Dream".as_bytes(), r.unwrap().body) });

    tokio::join!(instance_1, instance_2);

    runtime.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_functions_with_early_exit_are_handled() {
    let projects = vec![create_project("early-exit", None)];
    let (runtime, _, _) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: Cow::Borrowed("/"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed("Are You There?".as_bytes()),
    };

    use mu::runtime::error::*;
    match runtime
        .invoke_function(projects[0].id.clone(), request)
        .await
    {
        Err(Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(_))) => (),
        _ => panic!("Early exit function should fail to run"),
    }

    runtime.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn functions_with_limited_memory_wont_run() {
    let projects = vec![create_project(
        "memory-heavy",
        Some(byte_unit::Byte::from_unit(1.0, byte_unit::ByteUnit::MB).unwrap()),
    )];
    let (runtime, ..) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed(&[]),
    };

    let result = runtime
        .invoke_function(projects[0].id.clone(), request)
        .await;

    use mu::runtime::error::*;

    match result.err().unwrap() {
        Error::FunctionRuntimeError(FunctionRuntimeError::MaximumMemoryExceeded) => (),
        _ => panic!("Should panic!"),
    }

    runtime.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn functions_with_limited_memory_will_run_with_enough_memory() {
    let projects = vec![create_project(
        "memory-heavy",
        Some(byte_unit::Byte::from_unit(120.0, byte_unit::ByteUnit::MB).unwrap()),
    )];
    let (runtime, ..) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed("Test".as_bytes()),
    };

    runtime
        .invoke_function(projects[0].id.clone(), request)
        .then(|r| async move { assert_eq!("Hello Test, I ran!".as_bytes(), r.unwrap().body) })
        .await;

    runtime.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn function_usage_is_reported_correctly_1() {
    let projects = vec![create_project("hello-wasm", None)];
    let (runtime, _, usage_aggregator) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed("Chappy".as_bytes()),
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

    runtime.stop().await.unwrap();
}

#[tokio::test]
#[serial]
async fn function_usage_is_reported_correctly_2() {
    let projects = vec![create_project("database-heavy", None)];
    let (runtime, db_service, usage_aggregator) = create_runtime(&projects).await;

    let request = gateway::Request {
        method: mu_stack::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        data: Cow::Borrowed("Chappy".as_bytes()),
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

    println!("{:#?}", function_usage);

    assert!(function_usage.get(&UsageCategory::DBWrites).unwrap() == &10_001);

    assert!(function_usage.get(&UsageCategory::DBReads).unwrap() == &0);

    assert!(function_usage.get(&UsageCategory::DBStorage).unwrap() > &100);

    assert!(
        function_usage
            .get(&UsageCategory::FunctionMBInstructions)
            .unwrap()
            > &100
    );

    runtime.stop().await.unwrap();
}
