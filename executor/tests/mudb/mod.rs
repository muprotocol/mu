//! External cluster command
//! `tiup playground --mode tikv-slim --kv 3 --pd 3`
//! pd endpoints: 127.0.0.1:2379, 127.0.0.1:2382, 127.0.0.1:2384

// TODO
// use crate::infrastructure::config::*;
use assert_matches::assert_matches;
use futures::Future;
use mu::mudb_tikv::{db::DbImpl, embed_tikv::TikvRunner, error::*, types::*};
use mu_stack::StackID;
use rand::Rng;
use serial_test::serial;

fn stack_id() -> StackID {
    StackID::SolanaPublicKey([1; 32])
}

fn table_name_1() -> TableName {
    "a::a::a".into()
}

fn table_name_2() -> TableName {
    "a::a::b".into()
}

fn table_list() -> [TableName; 2] {
    [table_name_1(), table_name_2()]
}

async fn seed(db: &DbImpl, keys: [Key; 4], is_atomic: bool) {
    db.put(keys[0].clone(), values()[0].clone(), is_atomic)
        .await
        .unwrap();
    db.put(keys[1].clone(), values()[1].clone(), is_atomic)
        .await
        .unwrap();
    db.put(keys[2].clone(), values()[2].clone(), is_atomic)
        .await
        .unwrap();
    db.put(keys[3].clone(), values()[3].clone(), is_atomic)
        .await
        .unwrap();
}

fn keys(si: StackID, tl: [TableName; 2]) -> [Key; 4] {
    [
        Key {
            stack_id: si.clone(),
            table_name: tl[0].clone(),
            inner_key: vec![0, 0, 1],
        },
        Key {
            stack_id: si.clone(),
            table_name: tl[0].clone(),
            inner_key: vec![0, 1, 0],
        },
        Key {
            stack_id: si.clone(),
            table_name: tl[0].clone(),
            inner_key: vec![0, 1, 1],
        },
        Key {
            stack_id: si.clone(),
            table_name: tl[1].clone(),
            inner_key: vec![1, 0, 0],
        },
    ]
}

fn values() -> [Vec<u8>; 4] {
    [
        vec![10, 10, 10],
        vec![11, 11, 11],
        vec![12, 12, 12],
        vec![13, 13, 13],
    ]
}

async fn test_node<T>(
    db: DbImpl,
    stack_id: StackID,
    table_list: [TableName; 2],
    unique_key: Vec<u8>,
    keys: [Key; 4],
    is_atomic: bool,
    scans: T,
) where
    T: Future + Send + 'static,
{
    // db
    db.put_stack_manifest(stack_id.clone(), table_list.clone().into())
        .await
        .unwrap();
    let key = Key {
        stack_id: stack_id.clone(),
        table_name: table_list[0].clone(),
        inner_key: unique_key,
    };
    let value = "hello".to_string();
    // put
    db.put(key.clone(), value.clone().into(), is_atomic)
        .await
        .unwrap();
    // get
    let res = db.get(key.clone()).await.unwrap().unwrap();
    let res = String::from_utf8(res).unwrap();
    assert_eq!(res, value);
    // delete
    db.delete(key.clone(), false).await.unwrap();
    let res = db.get(key.clone()).await.unwrap();
    assert_eq!(res, None);
    // error table name dose not exist
    let err_key = Key {
        stack_id: stack_id.clone(),
        table_name: "no_existed_table".into(),
        inner_key: vec![],
    };
    let res = db.put(err_key.clone(), vec![], false).await;
    assert_matches!(res, Err(Error::StackIdOrTableDoseNotExist(_)));

    seed(&db, keys.clone(), is_atomic).await;

    // scan
    scans.await;

    let scan = Scan::ByInnerKey(
        keys[0].stack_id.clone(),
        keys[0].table_name.clone(),
        keys[0].inner_key.clone(),
    );
    let res = db.scan_keys(scan.clone(), 800).await.unwrap();
    assert_eq!(res, vec![keys[0].clone()]);

    let res = db.scan(scan, 800).await.unwrap();
    assert_eq!(res, vec![(keys[0].clone(), values()[0].clone())]);
}

async fn predictable_scan_for_keys_test(
    db: DbImpl,
    stack_id: StackID,
    table_list: [TableName; 2],
    keys: [Key; 4],
) {
    let scan = Scan::ByTableName(stack_id.clone(), table_list[0].clone());
    let res = db.scan_keys(scan, 800).await.unwrap();
    let x: Vec<Key> = keys
        .iter()
        .filter(|k| k.stack_id == stack_id && k.table_name == table_list[0])
        .map(Clone::clone)
        .collect();
    assert_eq!(res, x);

    let scan = Scan::ByTableName(stack_id.clone(), table_list[1].clone());
    let res = db.scan_keys(scan, 800).await.unwrap();
    let x: Vec<Key> = keys
        .iter()
        .filter(|k| k.stack_id == stack_id && k.table_name == table_list[1])
        .map(Clone::clone)
        .collect();
    assert_eq!(res, x);

    let scan = Scan::ByInnerKeyPrefix(stack_id.clone(), table_list[0].clone(), vec![0, 1]);
    let res = db.scan_keys(scan, 800).await.unwrap();
    let x: Vec<Key> = keys
        .iter()
        .filter(|k| {
            k.stack_id == stack_id
                && k.table_name == table_list[0]
                && k.inner_key.starts_with(&[0, 1])
        })
        .map(Clone::clone)
        .collect();
    assert_eq!(res, x);
}

async fn unpredictable_scan_for_keys_test(
    db: DbImpl,
    stack_id: StackID,
    table_list: [TableName; 2],
    keys: [Key; 4],
) {
    let scan = Scan::ByTableName(stack_id.clone(), table_list[0].clone());
    let res = db.scan_keys(scan, 800).await.unwrap();
    let x: Vec<Key> = keys
        .iter()
        .filter(|k| k.stack_id == stack_id && k.table_name == table_list[0])
        .map(Clone::clone)
        .collect();
    dbg!(res.clone());
    dbg!(x.clone());
    assert!(x.into_iter().all(|xp| res.contains(&xp)));

    let scan = Scan::ByTableName(stack_id.clone(), table_list[1].clone());
    let res2 = db.scan_keys(scan, 800).await.unwrap();
    let x: Vec<Key> = keys
        .iter()
        .filter(|k| k.stack_id == stack_id && k.table_name == table_list[1])
        .map(Clone::clone)
        .collect();
    dbg!(res2.clone());
    dbg!(x.clone());
    assert!(x.into_iter().all(|xp| res2.contains(&xp)));

    let scan = Scan::ByInnerKeyPrefix(stack_id.clone(), table_list[0].clone(), vec![0, 1]);
    let res = db.scan_keys(scan, 800).await.unwrap();
    let x: Vec<Key> = keys
        .iter()
        .filter(|k| {
            k.stack_id == stack_id
                && k.table_name == table_list[0]
                && k.inner_key.starts_with(&[0, 1])
        })
        .map(Clone::clone)
        .collect();
    assert!(x.into_iter().all(|xp| res.contains(&xp)));

    let scan = Scan::ByInnerKey(
        keys[0].stack_id.clone(),
        keys[0].table_name.clone(),
        keys[0].inner_key.clone(),
    );
    let res = db.scan_keys(scan.clone(), 800).await.unwrap();
    assert_eq!(res, vec![keys[0].clone()]);

    let res = db.scan(scan, 800).await.unwrap();
    assert_eq!(res, vec![(keys[0].clone(), values()[0].clone())]);
}

async fn table_list_test(db: DbImpl, tl: Vec<TableName>) {
    let table_names = db.table_list(stack_id().clone(), None).await.unwrap();
    assert_eq!(table_names, tl);
}

async fn single_node(db: DbImpl) {
    db.clear_all_data().await.unwrap();

    let db_clone = db.clone();
    test_node(
        db.clone(),
        stack_id(),
        table_list(),
        vec![1, 0],
        keys(stack_id(), table_list()),
        false,
        async move {
            predictable_scan_for_keys_test(
                db_clone,
                stack_id(),
                table_list(),
                keys(stack_id(), table_list()),
            )
            .await;
        },
    )
    .await;

    // scan table names
    table_list_test(db, table_list().into()).await;

    // TODO
    // let scan = Scan::ByTableName(stack_id.clone(), table_list[0].clone());
    // let res = db.scan_keys(scan, 32).await.unwrap();
    // assert_eq!(res.len(), 0);
    // let scan = Scan::ByTableName(stack_id.clone(), table_list[1].clone());
    // let res = db.scan_keys(scan, 32).await.unwrap();
    // assert_eq!(res.len(), 0);
    // let table_names = db.table_list(stack_id.clone(), None).await.unwrap();
    // assert_eq!(table_names.len(), 0);
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_single_node_without_embed() {
    single_node(
        DbImpl::new_without_embed_cluster(vec![
            "127.0.0.1:2379".try_into().unwrap(),
            "127.0.0.1:2382".try_into().unwrap(),
            "127.0.0.1:2384".try_into().unwrap(),
        ])
        .await
        .unwrap(),
    )
    .await;
}

// TODO
// #[tokio::test]
// #[serial]
// async fn test_single_node() {
//     let conf = initialize_config();
//     let node_address = NodeAddress {
//         address: "127.0.0.1".parse().unwrap(),
//         port: i,
//         generation: 1,
//     };
//     let known_node_conf = conf.2.clone();
//     let tikv_runner_conf = conf.3.clone();
//     let db = Db::new(node_address, known_node_conf, tikv_runner_conf)
//         .await
//         .unwrap();
//     single_node(db)
// }

fn rand_keys(si: StackID, tl: [TableName; 2]) -> [Key; 4] {
    [
        Key {
            stack_id: si.clone(),
            table_name: tl[0].clone(),
            inner_key: rand::thread_rng().gen::<[u8; 3]>().into(),
        },
        Key {
            stack_id: si.clone(),
            table_name: tl[0].clone(),
            inner_key: rand::thread_rng().gen::<[u8; 3]>().into(),
        },
        Key {
            stack_id: si.clone(),
            table_name: tl[0].clone(),
            inner_key: rand::thread_rng().gen::<[u8; 3]>().into(),
        },
        Key {
            stack_id: si.clone(),
            table_name: tl[1].clone(),
            inner_key: rand::thread_rng().gen::<[u8; 3]>().into(),
        },
    ]
}

async fn n_node_with_same_stack_id_and_tables(db: DbImpl, n: u8) {
    db.clear_all_data().await.unwrap();

    let mut handles = vec![];
    for i in 1..n {
        let db_clone = db.clone();
        let keys = rand_keys(stack_id(), table_list());
        let keys_clone = keys.clone();
        let f = test_node(
            db.clone(),
            stack_id(),
            table_list(),
            vec![i],
            keys.clone(),
            false,
            async move {
                unpredictable_scan_for_keys_test(db_clone, stack_id(), table_list(), keys_clone)
                    .await;
            },
        );
        handles.push(::tokio::spawn(f));
    }
    for h in handles {
        h.await.unwrap();
    }

    table_list_test(db, table_list().into()).await;
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_7_node_with_same_stack_id_and_tables() {
    n_node_with_same_stack_id_and_tables(
        DbImpl::new_without_embed_cluster(vec![
            "127.0.0.1:2379".try_into().unwrap(),
            "127.0.0.1:2382".try_into().unwrap(),
            "127.0.0.1:2384".try_into().unwrap(),
        ])
        .await
        .unwrap(),
        7,
    )
    .await;
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_50_node_with_same_stack_id_and_tables() {
    n_node_with_same_stack_id_and_tables(
        DbImpl::new_without_embed_cluster(vec![
            "127.0.0.1:2379".try_into().unwrap(),
            "127.0.0.1:2382".try_into().unwrap(),
            "127.0.0.1:2384".try_into().unwrap(),
        ])
        .await
        .unwrap(),
        50,
    )
    .await;
}

async fn n_node_with_same_stack_id(db: DbImpl, n: u8) {
    db.clear_all_data().await.unwrap();

    let mut handles = vec![];
    for i in 1..n {
        let tl = [format!("{}", i).into(), format!("{}", 100 + i).into()];
        let db_clone = db.clone();
        let f = test_node(
            db.clone(),
            stack_id(),
            tl.clone(),
            vec![i],
            rand_keys(stack_id(), tl.clone()),
            false,
            async move {
                predictable_scan_for_keys_test(
                    db_clone,
                    stack_id(),
                    table_list(),
                    keys(stack_id(), tl),
                )
                .await;
            },
        );
        handles.push(::tokio::spawn(f));
    }
    for h in handles {
        h.await.unwrap();
    }
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_7_node_with_same_stack_id() {
    n_node_with_same_stack_id(
        DbImpl::new_without_embed_cluster(vec![
            "127.0.0.1:2379".try_into().unwrap(),
            "127.0.0.1:2382".try_into().unwrap(),
            "127.0.0.1:2384".try_into().unwrap(),
        ])
        .await
        .unwrap(),
        7,
    )
    .await;
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_50_node_with_same_stack_id() {
    n_node_with_same_stack_id(
        DbImpl::new_without_embed_cluster(vec![
            "127.0.0.1:2379".try_into().unwrap(),
            "127.0.0.1:2382".try_into().unwrap(),
            "127.0.0.1:2384".try_into().unwrap(),
        ])
        .await
        .unwrap(),
        50,
    )
    .await;
}

async fn n_node_with_different_stack_id_and_tables(db: DbImpl, n: u8) {
    db.clear_all_data().await.unwrap();

    let mut handles = vec![];
    for i in 1..n {
        let si = StackID::SolanaPublicKey([i; 32]);
        let tl = [format!("{}", i).into(), format!("{}", 100 + i).into()];
        let db_clone = db.clone();
        let f = test_node(
            db.clone(),
            si.clone(),
            tl.clone(),
            vec![i],
            rand_keys(si.clone(), tl.clone()),
            false,
            async move {
                predictable_scan_for_keys_test(
                    db_clone,
                    si.clone(),
                    table_list(),
                    keys(si.clone(), tl),
                )
                .await;
            },
        );
        handles.push(::tokio::spawn(f));
    }
    for h in handles {
        h.await.unwrap();
    }
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_7_node_with_different_stack_id_and_tables() {
    n_node_with_different_stack_id_and_tables(
        DbImpl::new_without_embed_cluster(vec![
            "127.0.0.1:2379".try_into().unwrap(),
            "127.0.0.1:2382".try_into().unwrap(),
            "127.0.0.1:2384".try_into().unwrap(),
        ])
        .await
        .unwrap(),
        7,
    )
    .await;
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_50_node_with_different_stack_id_and_tables() {
    n_node_with_different_stack_id_and_tables(
        DbImpl::new_without_embed_cluster(vec![
            "127.0.0.1:2379".try_into().unwrap(),
            "127.0.0.1:2382".try_into().unwrap(),
            "127.0.0.1:2384".try_into().unwrap(),
        ])
        .await
        .unwrap(),
        50,
    )
    .await;
}

/// ##Test with external cluster,
/// To use test start external cluster as mensioned line 1,
/// comment #[ignore] and start testing.
#[tokio::test]
#[serial]
#[ignore]
async fn test_multi_node_with_manual_cluster_with_diffrent_endpoint_but_same_tikv() {
    let si = stack_id();
    let tl = table_list();
    let ks = keys(si.clone(), tl.clone());
    let vs = values();

    let db = DbImpl::new_without_embed_cluster(vec![
        "127.0.0.1:2379".try_into().unwrap(),
        // "127.0.0.1:2382".try_into().unwrap(),
        // "127.0.0.1:2384".try_into().unwrap(),
    ])
    .await
    .unwrap();

    let db2 = DbImpl::new_without_embed_cluster(vec![
        // "127.0.0.1:2379".try_into().unwrap(),
        "127.0.0.1:2382".try_into().unwrap(),
        // "127.0.0.1:2384".try_into().unwrap(),
    ])
    .await
    .unwrap();

    let db3 = DbImpl::new_without_embed_cluster(vec![
        // "127.0.0.1:2379".try_into().unwrap(),
        // "127.0.0.1:2382".try_into().unwrap(),
        "127.0.0.1:2384".try_into().unwrap(),
    ])
    .await
    .unwrap();

    for x in [&db, &db2, &db3] {
        x.clear_all_data().await.unwrap();
        x.put_stack_manifest(stack_id(), table_list().into())
            .await
            .unwrap();
    }

    db.put(ks[0].clone(), vs[0].clone(), false).await.unwrap();
    db2.put(ks[1].clone(), vs[1].clone(), false).await.unwrap();
    db3.put(ks[2].clone(), vs[2].clone(), false).await.unwrap();

    let x = db.get(ks[0].clone()).await.unwrap();
    let y = db2.get(ks[0].clone()).await.unwrap();
    let z = db3.get(ks[0].clone()).await.unwrap();
    assert_eq!(x, Some(vs[0].clone()));
    assert_eq!(y, Some(vs[0].clone()));
    assert_eq!(z, Some(vs[0].clone()));

    let x = db.get(ks[1].clone()).await.unwrap();
    let y = db2.get(ks[1].clone()).await.unwrap();
    let z = db3.get(ks[1].clone()).await.unwrap();
    assert_eq!(x, Some(vs[1].clone()));
    assert_eq!(y, Some(vs[1].clone()));
    assert_eq!(z, Some(vs[1].clone()));

    let x = db.get(ks[2].clone()).await.unwrap();
    let y = db2.get(ks[2].clone()).await.unwrap();
    let z = db3.get(ks[2].clone()).await.unwrap();
    assert_eq!(x, Some(vs[2].clone()));
    assert_eq!(y, Some(vs[2].clone()));
    assert_eq!(z, Some(vs[2].clone()));
}

// ============== OLD ================

// use mu::mudb::{service::*, Config, Result};
// use serde_json::json;
//
// async fn find_and_update_again(
//     db_service: &DatabaseManager,
//     database_id: &DatabaseID,
//     table_1: &str,
// ) -> Result<()> {
//     // find
//     db_service
//         .find_item(
//             database_id.clone(),
//             table_1.into(),
//             KeyFilter::Prefix("".into()),
//             json!({
//                 "num_item": { "$lt": 5 },
//                 "array_item": [2, 3]
//             })
//             .try_into()
//             .unwrap(),
//         )
//         .await?;

//     // update
//     db_service
//         .update_item(
//             database_id.clone(),
//             table_1.into(),
//             KeyFilter::Exact("ex::1".into()),
//             json!({
//                 "num_item": 1
//             })
//             .try_into()
//             .unwrap(),
//             json!({
//                 "$set": { "array_item.0": 10 },
//                 "$inc": { "array_item.1": 5 },
//             })
//             .try_into()
//             .unwrap(),
//         )
//         .await?;

//     Ok(())
// }

// #[tokio::test]
// #[serial]
// async fn test_mudb_service() {
//     let db_service = DatabaseManager::new().await.unwrap();

//     // init db

//     let database_id = DatabaseID {
//         db_name: "test_mudb_service".into(),
//         ..Default::default()
//     };

//     let conf = Config {
//         database_id: database_id.clone(),
//         ..Default::default()
//     };

//     db_service.create_db(conf).await.unwrap();

//     // create table 1

//     let table_1 = "table_1";

//     db_service
//         .create_table(database_id.clone(), table_1.into())
//         .await
//         .unwrap();

//     // create table 2

//     let table_2 = "table_2";

//     db_service
//         .create_table(database_id.clone(), table_2.into())
//         .await
//         .unwrap();

//     // insert one item

//     let value1 = json!({
//         "num_item": 1,
//         "array_item": [1, 2, 3, 4],
//         "obj_item": {
//             "in_1": "hello",
//             "in_2": "world",
//         }
//     })
//     .to_string();

//     let res1 = db_service
//         .insert_one_item(
//             database_id.clone(),
//             table_1.into(),
//             "ex::1".into(),
//             value1.clone(),
//         )
//         .await;

//     assert_eq!(res1, Ok("ex::1".to_string()));

//     // insert one item

//     let insert_one_res = db_service
//         .insert_one_item(
//             database_id.clone(),
//             table_2.into(),
//             "ex::5".into(),
//             json!({
//                 "array_item": ["h", "e", "l", "l", "o"],
//                 "obj_item": {
//                     "a": 10,
//                     "b": "hel",
//                 }
//             })
//             .to_string(),
//         )
//         .await;

//     dbg!(&insert_one_res);
//     println!("Inserted key: {:?}", insert_one_res);

//     // TODO
//     // // get table names
//     // assert_eq!(
//     //     db._table_names(),
//     //     Ok(vec!["table_1".to_string(), "table_2_auto_key".to_string()])
//     // );

//     // find

//     let find_res = db_service
//         .find_item(
//             database_id.clone(),
//             table_1.into(),
//             KeyFilter::Prefix("".into()),
//             json!({
//                 "num_item": { "$lt": 5 },
//                 "array_item": [2, 3]
//             })
//             .try_into()
//             .unwrap(),
//         )
//         .await
//         .unwrap();

//     dbg!(&find_res);
//     assert_eq!(find_res[0].0, "ex::1".to_string());
//     assert_eq!(find_res[0].1, value1);

//     // update

//     let update_res = db_service
//         .update_item(
//             database_id.clone(),
//             table_1.into(),
//             KeyFilter::Exact("ex::1".into()),
//             json!({
//                 "num_item": 1
//             })
//             .try_into()
//             .unwrap(),
//             json!({
//                 "$set": { "array_item.0": 10 },
//                 "$inc": { "array_item.1": 5 },
//             })
//             .try_into()
//             .unwrap(),
//         )
//         .await;

//     dbg!(&update_res);
//     assert_eq!(update_res.unwrap().len(), 1);

//     // concurrent db access (find/update) test

//     let mut handles = vec![];
//     (0..10).for_each(|_| {
//         let db_service_clone = db_service.clone();
//         let db_id_clone = database_id.clone();
//         let handle = ::tokio::spawn(async move {
//             find_and_update_again(&db_service_clone, &db_id_clone, table_1).await
//         });
//         handles.push(handle);
//     });

//     for handle in handles {
//         handle.await.unwrap().unwrap();
//     }

//     // delete

//     let del_res = db_service
//         .delete_item(
//             database_id.clone(),
//             table_2.into(),
//             KeyFilter::Prefix("".into()),
//             json!({
//                 "obj_item": { "a": 10 }
//             })
//             .try_into()
//             .unwrap(),
//         )
//         .await;

//     dbg!(&del_res);
//     assert_eq!(del_res.unwrap().len(), 1);

//     // delete table 1

//     let res = db_service
//         .delete_table(database_id.clone(), table_1.into())
//         .await;

//     assert_eq!(
//         res,
//         Ok(Some(TableDescription {
//             table_name: "table_1".into(),
//         }))
//     );

//     // delete table 2

//     let res = db_service
//         .delete_table(database_id.clone(), table_2.into())
//         .await;

//     assert_eq!(
//         res,
//         Ok(Some(TableDescription {
//             table_name: "table_2".into(),
//         }))
//     );

//     // drop db
//     db_service.drop_db(&database_id).await.unwrap();
// }
