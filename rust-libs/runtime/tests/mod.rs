use std::{borrow::Cow, collections::HashMap, path::Path};

use futures::FutureExt;
use itertools::Itertools;

use mu_runtime::*;
use mu_stack::{self, AssemblyID, StackID};
use musdk_common::{Header, Status};

use crate::utils::{create_runtime, Project};

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
        path: Path::new(&format!("tests/funcs/{name}")).into(),
        id: AssemblyID {
            stack_id: StackID::SolanaPublicKey(rand::random()),
            assembly_name: name.into(),
        },
        memory_limit,
        functions,
    }
}

fn make_request<'a>(
    body: Cow<'a, [u8]>,
    headers: Vec<Header<'a>>,
    path_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
    query_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
) -> musdk_common::Request<'a> {
    musdk_common::Request {
        method: musdk_common::HttpMethod::Get,
        headers,
        body,
        path_params,
        query_params,
    }
}

#[tokio::test]
async fn test_simple_func() {
    let projects = vec![create_project("hello-wasm", &["say_hello"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Chappy"),
        vec![],
        HashMap::new(),
        HashMap::new(),
    );

    let resp = runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .await
        .unwrap();

    assert_eq!(
        "Hello Chappy, welcome to MuRuntime".as_bytes(),
        resp.body.as_ref()
    );

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

// #[tokio::test]
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
async fn can_run_multiple_instance_of_the_same_function() {
    let projects = vec![create_project("hello-wasm", &["say_hello"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let make_request = |name: &'static str| {
        make_request(
            Cow::Borrowed(name.as_bytes()),
            vec![],
            HashMap::new(),
            HashMap::new(),
        )
    };

    let function_id = projects[0].function_id(0).unwrap();

    let instance_1 = runtime
        .invoke_function(function_id.clone(), make_request("Mathew"))
        .then(|r| async move {
            assert_eq!(
                "Hello Mathew, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    let instance_2 = runtime
        .invoke_function(function_id.clone(), make_request("Morpheus"))
        .then(|r| async move {
            assert_eq!(
                "Hello Morpheus, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    let instance_3 = runtime
        .invoke_function(function_id, make_request("Unity"))
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
async fn can_run_instances_of_different_functions() {
    let projects = vec![
        create_project("hello-wasm", &["say_hello"], None),
        create_project("calc-func", &["add_one"], None),
    ];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let make_request = |body| make_request(body, vec![], HashMap::new(), HashMap::new());

    let instance_1 = runtime
        .invoke_function(
            projects[0].function_id(0).unwrap(),
            make_request(Cow::Borrowed(b"Mathew")),
        )
        .then(|r| async move {
            assert_eq!(
                "Hello Mathew, welcome to MuRuntime".as_bytes(),
                r.unwrap().body.as_ref()
            )
        });

    let number = 2023u32;
    let instance_2 = runtime
        .invoke_function(
            projects[1].function_id(0).unwrap(),
            make_request(Cow::Owned(number.to_be_bytes().to_vec())),
        )
        .then(|r| async move {
            let bytes = r.unwrap().body;
            assert_eq!(
                number + 2,
                u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            )
        });

    tokio::join!(instance_1, instance_2);

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
async fn unclean_termination_is_handled() {
    use mu_runtime::error::*;

    let projects = vec![create_project("unclean-termination", &["say_hello"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(Cow::Borrowed(b""), vec![], HashMap::new(), HashMap::new());

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
async fn functions_with_limited_memory_wont_run() {
    use mu_runtime::error::*;

    let projects = vec![create_project(
        "hello-wasm",
        &["memory_heavy"],
        Some(byte_unit::Byte::from_unit(1.0, byte_unit::ByteUnit::MB).unwrap()),
    )];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Fred"),
        vec![],
        HashMap::new(),
        HashMap::new(),
    );

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
async fn functions_with_limited_memory_will_run_with_enough_memory() {
    let projects = vec![create_project(
        "hello-wasm",
        &["memory_heavy"],
        Some(byte_unit::Byte::from_unit(120.0, byte_unit::ByteUnit::MB).unwrap()),
    )];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Fred"),
        vec![],
        HashMap::new(),
        HashMap::new(),
    );

    runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .then(|r| async move { assert_eq!(b"Fred", r.unwrap().body.as_ref()) })
        .await;

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
async fn function_usage_is_reported_correctly_1() {
    let projects = vec![create_project("hello-wasm", &["say_hello"], None)];
    let (runtime, db_manager, usages) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Chappy"),
        vec![],
        HashMap::new(),
        HashMap::new(),
    );

    let function_id = projects[0].function_id(0).unwrap();

    runtime
        .invoke_function(function_id.clone(), request)
        .await
        .unwrap();

    let usages = usages.lock().await;

    let Usage {
        db_weak_reads,
        db_strong_reads,
        db_weak_writes,
        db_strong_writes,
        function_instructions,
        memory_megabytes,
    } = usages.get(function_id.stack_id()).unwrap();

    assert_eq!(*db_weak_writes, 0);
    assert_eq!(*db_weak_reads, 0);
    assert_eq!(*db_strong_writes, 0);
    assert_eq!(*db_strong_reads, 0);
    assert!(*function_instructions > 0);
    assert_eq!(*memory_megabytes, 100);

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

//#[tokio::test]
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
async fn failing_function_should_not_hang() {
    use mu_runtime::error::*;
    let projects = vec![create_project("hello-wasm", &["failing"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Chappy"),
        vec![],
        HashMap::new(),
        HashMap::new(),
    );

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

#[tokio::test]
async fn json_body_request_and_response() {
    use serde::{Deserialize, Serialize};

    let projects = vec![create_project("multi-body", &["json_body"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    #[derive(Serialize)]
    pub struct Form {
        pub username: String,
        pub password: String,
    }

    #[derive(Deserialize, PartialEq, Eq, Debug)]
    pub struct Response {
        pub token: String,
        pub ttl: u64,
    }

    let form = serde_json::to_vec(&Form {
        username: "John".into(),
        password: "12345".into(),
    })
    .unwrap();

    let expected_response = Response {
        token: "token_for_John_12345".into(),
        ttl: 14011018,
    };

    let request = make_request(
        Cow::Borrowed(&form),
        vec![Header {
            name: Cow::Borrowed("content-type"),
            value: Cow::Borrowed("application/json; charset=utf-8"),
        }],
        HashMap::new(),
        HashMap::new(),
    );

    runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .then(|r| async move {
            let r = r.unwrap();
            assert_eq!(Status::Ok, r.status);
            assert_eq!(
                expected_response,
                serde_json::from_slice(r.body.as_ref()).unwrap()
            )
        })
        .await;

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
async fn string_body_request_and_response() {
    let projects = vec![create_project("multi-body", &["string_body"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Due"),
        vec![],
        HashMap::new(),
        HashMap::new(),
    );

    runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .then(|r| async move {
            let r = r.unwrap();
            assert_eq!(Status::Ok, r.status);
            assert_eq!(b"Hello Due, got your message", r.body.as_ref());
        })
        .await;

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
async fn string_body_request_and_response_fails_with_incorrect_charset() {
    let projects = vec![create_project("multi-body", &["string_body"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Due"),
        vec![Header {
            name: Cow::Borrowed("content-type"),
            value: Cow::Borrowed("text/plain; charset=windows-12345"),
        }],
        HashMap::new(),
        HashMap::new(),
    );

    runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .then(|r| async move {
            let r = r.unwrap();
            assert_eq!(Status::BadRequest, r.status);
            assert_eq!(b"unsupported charset: windows-12345", r.body.as_ref());
        })
        .await;

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
async fn string_body_request_and_response_do_not_care_for_content_type() {
    let projects = vec![create_project("multi-body", &["string_body"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Due"),
        vec![Header {
            name: Cow::Borrowed("content-type"),
            value: Cow::Borrowed("application/json; charset=utf-8"),
        }],
        HashMap::new(),
        HashMap::new(),
    );

    runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .then(|r| async move {
            let r = r.unwrap();
            assert_eq!(Status::Ok, r.status);
            assert_eq!(b"Hello Due, got your message", r.body.as_ref());
        })
        .await;

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}

#[tokio::test]
async fn can_access_path_params() {
    let projects = vec![create_project("hello-wasm", &["path_params"], None)];
    let (runtime, db_manager, _) = create_runtime(&projects).await;

    let request = make_request(
        Cow::Borrowed(b"Due"),
        vec![],
        [("type".into(), "users".into()), ("id".into(), "13".into())].into(),
        HashMap::new(),
    );

    let expected_response = request
        .path_params
        .iter()
        .sorted_by(|i, j| i.0.cmp(j.0))
        .map(|(k, v)| format!("{k}:{v}"))
        .reduce(|i, j| format!("{i},{j}"))
        .unwrap_or_else(|| "".into());

    runtime
        .invoke_function(projects[0].function_id(0).unwrap(), request)
        .then(|r| async move {
            let r = r.unwrap();
            assert_eq!(Status::Ok, r.status);
            assert_eq!(expected_response.as_bytes(), r.body.as_ref());
        })
        .await;

    runtime.stop().await.unwrap();
    db_manager.stop_embedded_cluster().await.unwrap();
}
