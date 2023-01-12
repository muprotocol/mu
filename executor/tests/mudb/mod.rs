use assert_matches::assert_matches;
use futures::Future;
use mu::{
    mudb_tikv::{db::DbImpl, error::*, types::*, PdConfig, TikvConfig, TikvRunnerConfig},
    network::{gossip::KnownNodeConfig, NodeAddress},
};
use mu_stack::StackID;
use rand::Rng;
use serial_test::serial;
use std::fs;
use std::net::IpAddr;

const TEST_DATA_DIR: &str = "tests/mudb/test_data";

fn clean_data_dir() {
    fs::remove_dir_all(TEST_DATA_DIR).unwrap_or_else(|why| {
        println!("{} {:?}", TEST_DATA_DIR, why.kind());
    });
}

fn stack_id() -> StackID {
    StackID::SolanaPublicKey([1; 32])
}

fn table_name_1() -> TableName {
    "a::a::a".try_into().unwrap()
}

fn table_name_2() -> TableName {
    "a::a::b".try_into().unwrap()
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
        table_name: "no_existed_table".try_into().unwrap(),
        inner_key: vec![],
    };
    let res = db.put(err_key.clone(), vec![], false).await;
    assert_matches!(res, Err(Error::StackIdOrTableDoseNotExist(_)));

    seed(&db, keys.clone(), is_atomic).await;

    // scan
    scans.await;
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
}

fn make_node_address(port: u16) -> NodeAddress {
    NodeAddress {
        address: "127.0.0.1".parse().unwrap(),
        port,
        generation: 1,
    }
}
fn make_tikv_runner_conf(peer_port: u16, client_port: u16, tikv_port: u16) -> TikvRunnerConfig {
    let localhost: IpAddr = "127.0.0.1".parse().unwrap();
    TikvRunnerConfig {
        pd: PdConfig {
            peer_url: IpAndPort {
                address: localhost.clone(),
                port: peer_port,
            },
            client_url: IpAndPort {
                address: localhost.clone(),
                port: client_port,
            },
            data_dir: format!("{TEST_DATA_DIR}/pd_data_dir_{peer_port}"),
            log_file: Some(format!("{TEST_DATA_DIR}/pd_log_{peer_port}")),
        },
        node: TikvConfig {
            cluster_url: IpAndPort {
                address: localhost.clone(),
                port: tikv_port,
            },
            data_dir: format!("{TEST_DATA_DIR}/tikv_data_dir_{tikv_port}"),
            log_file: Some(format!("{TEST_DATA_DIR}/tikv_log_{tikv_port}")),
        },
    }
}
fn make_known_node_conf(gossip_port: u16, pd_port: u16) -> KnownNodeConfig {
    KnownNodeConfig {
        address: "127.0.0.1".parse().unwrap(),
        gossip_port,
        pd_port,
    }
}

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

async fn n_node_with_same_stack_id_and_tables(dbs: Vec<DbImpl>) {
    let db = dbs[0].clone();
    db.clear_all_data().await.unwrap();

    let mut handles = vec![];
    for (i, db) in dbs.into_iter().enumerate() {
        let db_clone = db.clone();
        let keys = rand_keys(stack_id(), table_list());
        let keys_clone = keys.clone();
        let f = test_node(
            db.clone(),
            stack_id(),
            table_list(),
            vec![i as u8],
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

async fn n_node_with_same_stack_id(dbs: Vec<DbImpl>) {
    let db = dbs[0].clone();
    db.clear_all_data().await.unwrap();

    let mut handles = vec![];
    for (i, db) in dbs.into_iter().enumerate() {
        let tl = [
            format!("{}", i).try_into().unwrap(),
            format!("{}", 100 + i).try_into().unwrap(),
        ];
        let db_clone = db.clone();
        let f = test_node(
            db.clone(),
            stack_id(),
            tl.clone(),
            vec![i as u8],
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

async fn n_node_with_different_stack_id_and_tables(dbs: Vec<DbImpl>) {
    let db = dbs[0].clone();
    db.clear_all_data().await.unwrap();

    let mut handles = vec![];
    for (i, db) in dbs.into_iter().enumerate() {
        let i = i as u8;
        let si = StackID::SolanaPublicKey([i; 32]);
        let tl = [
            format!("{}", i).try_into().unwrap(),
            format!("{}", 100 + i).try_into().unwrap(),
        ];
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

async fn make_db_with_external_cluster() -> DbImpl {
    DbImpl::new_with_external_cluster(vec![
        "127.0.0.1:2379".try_into().unwrap(),
        "127.0.0.1:2382".try_into().unwrap(),
        "127.0.0.1:2384".try_into().unwrap(),
    ])
    .await
    .unwrap()
}

async fn make_3_dbs() -> Vec<DbImpl> {
    let mut handles = vec![];
    let h = ::tokio::spawn(async move {
        let node_address = make_node_address(2800);
        let tikv_runner_conf = make_tikv_runner_conf(2380, 2379, 20160);
        let known_node_conf = vec![
            make_known_node_conf(2801, 2381),
            make_known_node_conf(2802, 2383),
        ];
        DbImpl::new_with_embedded_cluster(node_address, known_node_conf, tikv_runner_conf)
            .await
            .unwrap()
    });
    handles.push(h);

    let h = ::tokio::spawn(async move {
        let node_address = make_node_address(2801);
        let tikv_runner_conf = make_tikv_runner_conf(2381, 2382, 20161);
        let known_node_conf = vec![
            make_known_node_conf(2800, 2380),
            make_known_node_conf(2802, 2383),
        ];
        DbImpl::new_with_embedded_cluster(node_address, known_node_conf, tikv_runner_conf)
            .await
            .unwrap()
    });
    handles.push(h);

    let h = ::tokio::spawn(async move {
        let node_address = make_node_address(2802);
        let tikv_runner_conf = make_tikv_runner_conf(2383, 2384, 20162);
        let known_node_conf = vec![
            make_known_node_conf(2800, 2380),
            make_known_node_conf(2801, 2381),
        ];
        DbImpl::new_with_embedded_cluster(node_address, known_node_conf, tikv_runner_conf)
            .await
            .unwrap()
    });
    handles.push(h);

    let mut dbs = vec![];
    for h in handles {
        let db = h.await.unwrap();
        dbs.push(db);
    }

    dbs
}

#[tokio::test]
#[serial]
async fn test_single_node_with_embed_and_stop() {
    clean_data_dir();

    let node_address = make_node_address(2803);
    let known_node_conf = vec![];
    let tikv_runner_conf = make_tikv_runner_conf(2385, 2386, 20163);
    let db = DbImpl::new_with_embedded_cluster(node_address, known_node_conf, tikv_runner_conf)
        .await
        .unwrap();

    single_node(db.clone()).await;
    db.stop_embed_cluster().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_3_node_with_embed_same_stack_id_same_table_then_stop() {
    clean_data_dir();

    let dbs = make_3_dbs().await;

    n_node_with_same_stack_id_and_tables(dbs.clone()).await;

    for db in dbs {
        db.stop_embed_cluster().await.unwrap();
    }
}

#[tokio::test]
#[serial]
async fn test_3_node_with_embed_same_stack_id_then_stop() {
    clean_data_dir();

    let dbs = make_3_dbs().await;

    n_node_with_same_stack_id(dbs.clone()).await;

    for db in dbs {
        db.stop_embed_cluster().await.unwrap();
    }
}

#[tokio::test]
#[serial]
async fn test_3_node_with_embed_different_stack_id_and_tables_then_stop() {
    clean_data_dir();

    let dbs = make_3_dbs().await;

    n_node_with_different_stack_id_and_tables(dbs.clone()).await;

    for db in dbs {
        db.stop_embed_cluster().await.unwrap();
    }
}

// # Test with external cluster,
// To use test start external cluster as below then
// comment #[ignore] and start testing.
//
// # External cluster command
// `tiup playground --mode tikv-slim --kv 3 --pd 3`
// pd endpoints: 127.0.0.1:2379, 127.0.0.1:2382, 127.0.0.1:2384

#[tokio::test]
#[serial]
#[ignore]
async fn test_single_node_without_embed() {
    single_node(make_db_with_external_cluster().await).await;
}

#[tokio::test]
#[serial]
#[ignore]
async fn test_50_node_with_same_stack_id_and_tables() {
    let db = make_db_with_external_cluster().await;
    n_node_with_same_stack_id_and_tables((0..50).map(|_| db.clone()).collect()).await;
}

#[tokio::test]
#[serial]
#[ignore]
async fn test_50_node_with_same_stack_id() {
    let db = make_db_with_external_cluster().await;
    n_node_with_same_stack_id((0..50).map(|_| db.clone()).collect()).await;
}

#[tokio::test]
#[serial]
#[ignore]
async fn test_50_node_with_different_stack_id_and_tables() {
    let db = make_db_with_external_cluster().await;
    n_node_with_different_stack_id_and_tables((0..50).map(|_| db.clone()).collect()).await;
}

#[tokio::test]
#[serial]
#[ignore]
async fn test_multi_node_with_manual_cluster_with_different_endpoint_but_same_tikv() {
    let si = stack_id();
    let tl = table_list();
    let ks = keys(si.clone(), tl.clone());
    let vs = values();

    let db = DbImpl::new_with_external_cluster(vec![
        "127.0.0.1:2379".try_into().unwrap(),
        // "127.0.0.1:2382".try_into().unwrap(),
        // "127.0.0.1:2384".try_into().unwrap(),
    ])
    .await
    .unwrap();

    let db2 = DbImpl::new_with_external_cluster(vec![
        // "127.0.0.1:2379".try_into().unwrap(),
        "127.0.0.1:2382".try_into().unwrap(),
        // "127.0.0.1:2384".try_into().unwrap(),
    ])
    .await
    .unwrap();

    let db3 = DbImpl::new_with_external_cluster(vec![
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
