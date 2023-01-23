use futures::FutureExt;
use mu::{runtime::types::AssemblyID, stack::usage_aggregator::UsageCategory};
use mu_stack::{self, StackID};
use serial_test::serial;
use std::{borrow::Cow, collections::HashMap, path::Path};

use crate::runtime::utils::{create_runtime, Project};

mod providers;
mod utils;

pub fn create_project<'a>(
    name: &'a str,
    functions: &'a [&'a str],
    memory_limit: Option<byte_unit::Byte>,
) -> Project<'a> {
    let memory_limit = memory_limit
        .unwrap_or_else(|| byte_unit::Byte::from_unit(100.0, byte_unit::ByteUnit::MB).unwrap());

    Project {
        name,
        path: Path::new(&format!("tests/runtime/funcs/{name}")).into(),
        id: AssemblyID {
            stack_id: StackID::SolanaPublicKey(rand::random()),
            assembly_name: name.into(),
        },
        memory_limit,
        functions,
    }
}

#[tokio::test]
#[serial]
async fn test_simple_func() {
    env_logger::init();

    let projects = vec![create_project("hello-wasm", &["say_hello"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed("Chappy".as_bytes()),
    };

    let resp = runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .await
        .unwrap();

    assert_eq!(
        "Hello Chappy, welcome to MuRuntime".as_bytes(),
        resp.body.into_owned()
    );

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

// #[tokio::test]
// #[serial]
// async fn can_query_mudb() {
//     let projects = vec![create_project("hello-mudb", None)];
//     let (runtime, db_service, _) = create_runtime(&projects).await;

//     let database_id = DatabaseID {
//         stack_id: projects[0].id.stack_id,
//         db_name: "my_db".into(),
//     };

//     create_db_if_not_exist(db_service, database_id)
//         .await
//         .unwrap();

//     let request = musdk_common::Request {
//         method: musdk_common::HttpMethod::Get,
//         path: Cow::Borrowed("/get_name"),
//         query: HashMap::new(),
//         headers: Vec::new(),
//         body: Cow::Borrowed("Dream".as_bytes()),
//     };

//     let resp = runtime
//         .invoke_function(projects[0].id.clone(), request)
//         .await
//         .unwrap();

//     assert_eq!(Cow::Borrowed("Hello Dream".as_bytes()), resp.body);
//     runtime.stop().await.unwrap();
// }

#[tokio::test]
#[serial]
async fn can_run_multiple_instance_of_the_same_function() {
    let projects = vec![create_project("hello-wasm", &["say_hello"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let make_request = |name| musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed(name),
    };

    let function_id = projects[0].function_id(0).unwrap();

    let instance_1 = runtime
        .invoke_function(function_id.clone(), make_request("Mathew".as_bytes()))
        .then(|r| async move {
            assert_eq!(
                "Hello Mathew, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    let instance_2 = runtime
        .invoke_function(function_id.clone(), make_request("Morpheus".as_bytes()))
        .then(|r| async move {
            assert_eq!(
                "Hello Morpheus, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    let instance_3 = runtime
        .invoke_function(function_id, make_request("Unity".as_bytes()))
        .then(|r| async move {
            assert_eq!(
                "Hello Unity, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    tokio::join!(instance_1, instance_2, instance_3);

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
#[serial]
async fn can_run_instances_of_different_functions() {
    let projects = vec![
        create_project("hello-wasm", &["say_hello"], None),
        create_project(
            "memory-heavy",
            &["say_hello"],
            Some(byte_unit::Byte::from_unit(120.0, byte_unit::ByteUnit::MB).unwrap()),
        ),
    ];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let make_request = |name| musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed(name),
    };

    let instance_1 = runtime
        .invoke_function(
            projects[0].function_id(0).unwrap(),
            make_request("Mathew".as_bytes()),
        )
        .then(|r| async move {
            assert_eq!(
                "Hello Mathew, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    let instance_2 = runtime
        .invoke_function(
            projects[0].function_id(0).unwrap(),
            make_request("Dream".as_bytes()),
        )
        .then(|r| async move {
            assert_eq!(
                "Hello Dream, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    tokio::join!(instance_1, instance_2);

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
#[serial]
async fn unclean_termination_is_handled() {
    let projects = vec![create_project("unclean-termination", &["say_hello"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: Cow::Borrowed("/"),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed("Are You There?".as_bytes()),
    };

    use mu::runtime::error::*;
    match runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .await
    {
        Err(Error::FunctionDidntTerminateCleanly) => (),
        _ => panic!("Unclean exit function should fail to run"),
    }

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
#[serial]
async fn functions_with_limited_memory_wont_run() {
    use mu::runtime::error::*;

    let projects = vec![create_project(
        "memory-heavy",
        &["say_hello"],
        Some(byte_unit::Byte::from_unit(1.0, byte_unit::ByteUnit::MB).unwrap()),
    )];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed(&[]),
    };

    let result = runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .await;

    match result.err().unwrap() {
        Error::FunctionRuntimeError(FunctionRuntimeError::MaximumMemoryExceeded) => (),
        _ => panic!("Should panic!"),
    }

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
#[serial]
async fn functions_with_limited_memory_will_run_with_enough_memory() {
    let projects = vec![create_project(
        "memory-heavy",
        &["say_hello"],
        Some(byte_unit::Byte::from_unit(120.0, byte_unit::ByteUnit::MB).unwrap()),
    )];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed("Fred".as_bytes()),
    };

    runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .then(|r| async move {
            assert_eq!(
                "Hello Fred, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        })
        .await;

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
#[serial]
async fn function_usage_is_reported_correctly_1() {
    let projects = vec![create_project("hello-wasm", &["say_hello"], None)];
    let (runtime, db_manager, usage_aggregator) = create_runtime(&projects).await;

    let request = musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: Cow::Borrowed("/get_name"),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed("Chappy".as_bytes()),
    };

    let function_id = projects[0].function_id(0).unwrap();

    runtime
        .invoke_function(function_id.clone(), request)
        .await
        .unwrap();

    let usages = usage_aggregator.get_and_reset_usages().await.unwrap();
    let function_usage = usages.get(function_id.stack_id()).unwrap();

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
    db_manager.stop_embedded_cluster().await.unwrap();
}

//#[tokio::test]
//#[serial]
//async fn function_usage_is_reported_correctly_2() {
//    let projects = vec![create_project("database-heavy", None)];
//    let (runtime, db_service, usage_aggregator) = create_runtime(&projects).await;
//
//    let request = musdk_common::Request {
//        method: musdk_common::HttpMethod::Get,
//        path: Cow::Borrowed("/get_name"),
//        query: HashMap::new(),
//        headers: Vec::new(),
//        body: Cow::Borrowed("Chappy".as_bytes()),
//    };
//
//    let database_id = DatabaseID {
//        stack_id: projects[0].id.stack_id,
//        db_name: "my_db".into(),
//    };
//
//    create_db_if_not_exist(db_service, database_id)
//        .await
//        .unwrap();
//
//    runtime
//        .invoke_function(projects[0].id.clone(), request)
//        .await
//        .unwrap();
//
//    let usages = usage_aggregator.get_and_reset_usages().await.unwrap();
//    let function_usage = usages.get(&projects[0].id.stack_id).unwrap();
//
//    assert!(function_usage.get(&UsageCategory::DBWrites).unwrap() == &10_001);
//
//    assert!(function_usage.get(&UsageCategory::DBReads).unwrap() == &0);
//
//    assert!(function_usage.get(&UsageCategory::DBStorage).unwrap() > &100);
//
//    assert!(
//        function_usage
//            .get(&UsageCategory::FunctionMBInstructions)
//            .unwrap()
//            > &100
//    );
//
//    runtime.stop().await.unwrap();
//}

#[tokio::test]
#[serial]
async fn failing_function_should_not_hang() {
    use mu::runtime::error::*;
    let projects = vec![create_project("failing", &["say_hello"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        path: "/get_name".into(),
        query: HashMap::new(),
        headers: Vec::new(),
        body: Cow::Borrowed(b"Chappy"),
    };

    let result = runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .await;

    match result.err().unwrap() {
        Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(_)) => (),
        _ => panic!("function should have been exited early!"),
    }

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}
