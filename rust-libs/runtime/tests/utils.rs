use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use mu_db::{DbManager, IpAndPort, NodeAddress, PdConfig, TikvConfig, TikvRunnerConfig};
use mu_runtime::{start, AssemblyDefinition, Notification, Runtime, RuntimeConfig, Usage};
use mu_stack::{AssemblyID, AssemblyRuntime, FunctionID, StackID};
use musdk_common::Header;

// Add test project names (directory name) in this array to build them when testing
const TEST_PROJECTS: &[&str] = &[
    "hello-wasm",
    "calc-func",
    "multi-body",
    "unclean-termination",
];

pub struct Project<'a> {
    pub id: AssemblyID,
    pub name: &'a str,
    pub path: PathBuf,
    pub memory_limit: byte_unit::Byte,
    pub functions: &'a [&'a str],
}

impl<'a> Project<'a> {
    pub fn wasm_module_path(&self) -> PathBuf {
        self.path
            .join("target/wasm32-wasi/release/")
            .join(format!("{}.wasm", self.name))
    }

    pub fn function_id(&self, index: usize) -> Option<FunctionID> {
        if index <= self.functions.len() {
            Some(FunctionID {
                assembly_id: self.id.clone(),
                function_name: self.functions[index].to_string(),
            })
        } else {
            None
        }
    }
}

pub async fn read_wasm_functions<'a>(
    projects: &'a [Project<'a>],
) -> Result<HashMap<AssemblyID, AssemblyDefinition>> {
    let mut results = HashMap::new();

    for project in projects {
        let source = std::fs::read(project.wasm_module_path())?;

        results.insert(
            project.id.clone(),
            AssemblyDefinition::try_new(
                project.id.clone(),
                source.into(),
                AssemblyRuntime::Wasi1_0,
                [],
                project.memory_limit,
            )?,
        );
    }

    Ok(results)
}

pub mod fixture {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use test_context::{AsyncTestContext, TestContext};

    pub static DID_INSTALL_WASM32_TARGET_RUN: AtomicBool = AtomicBool::new(false);
    pub static DID_BUILD_TEST_FUNCS_FIXTURE_RUN: AtomicBool = AtomicBool::new(false);

    fn install_wasm32_target() {
        if !DID_INSTALL_WASM32_TARGET_RUN.load(Ordering::Relaxed) {
            println!("Installing wasm32-wasi target.");
            Command::new("rustup")
                .args(["target", "add", "wasm32-wasi"])
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
            DID_INSTALL_WASM32_TARGET_RUN.store(true, Ordering::Relaxed);
        }
    }

    fn build_test_funcs() {
        if !DID_BUILD_TEST_FUNCS_FIXTURE_RUN.load(Ordering::Relaxed) {
            println!("Building test functions.");
            for name in TEST_PROJECTS {
                let project_dir = format!("tests/funcs/{name}");
                Command::new("cargo")
                    .current_dir(project_dir)
                    .env_remove("CARGO_TARGET_DIR")
                    .arg("build")
                    .args(["--release", "--target", "wasm32-wasi"])
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();
                DID_BUILD_TEST_FUNCS_FIXTURE_RUN.store(true, Ordering::Relaxed);
            }
        }
    }

    pub struct TempDir(PathBuf);

    impl TestContext for TempDir {
        fn setup() -> Self {
            TempDir::new()
        }

        fn teardown(self) {
            std::fs::remove_dir_all(self.0).unwrap();
        }
    }

    impl TempDir {
        pub fn new() -> Self {
            TempDir(std::env::temp_dir().join(TempDir::rand_dir_name()))
        }

        pub fn get_rand_sub_dir(&self, prefix: Option<&str>) -> PathBuf {
            let name = format!("{}{}", prefix.unwrap_or(""), Self::rand_dir_name());
            self.0.join(name)
        }

        fn rand_dir_name() -> String {
            let rand: [u8; 5] = rand::random();
            rand.into_iter()
                .fold(String::new(), |a, i| format!("{a}{i}"))
        }
    }

    impl Default for TempDir {
        fn default() -> Self {
            Self::new()
        }
    }

    pub struct DBManagerFixture {
        db_manager: Box<dyn DbManager>,
        data_dir: TempDir,
    }

    #[async_trait]
    impl AsyncTestContext for DBManagerFixture {
        async fn setup() -> Self {
            let data_dir = TempDir::setup();
            let localhost = IpAddr::V4(Ipv4Addr::LOCALHOST);

            let node_address = NodeAddress {
                address: localhost,
                port: 12803,
            };

            let tikv_config = TikvRunnerConfig {
                pd: PdConfig {
                    peer_url: IpAndPort {
                        address: localhost,
                        port: 12385,
                    },
                    client_url: IpAndPort {
                        address: localhost,
                        port: 12386,
                    },
                    data_dir: data_dir
                        .get_rand_sub_dir(Some("pd_data_dir"))
                        .display()
                        .to_string(),
                    log_file: Some(
                        data_dir
                            .get_rand_sub_dir(Some("pd_log"))
                            .display()
                            .to_string(),
                    ),
                },
                node: TikvConfig {
                    cluster_url: IpAndPort {
                        address: localhost,
                        port: 20163,
                    },
                    data_dir: data_dir
                        .get_rand_sub_dir(Some("tikv_data_dir"))
                        .display()
                        .to_string(),
                    log_file: Some(
                        data_dir
                            .get_rand_sub_dir(Some("tikv_log"))
                            .display()
                            .to_string(),
                    ),
                },
            };

            Self {
                db_manager: mu_db::new_with_embedded_cluster(node_address, vec![], tikv_config)
                    .await
                    .unwrap(),

                data_dir,
            }
        }

        async fn teardown(self) {
            self.db_manager.stop_embedded_cluster().await.unwrap();
            self.data_dir.teardown()
        }
    }

    pub struct RuntimeFixture {
        pub runtime: Box<dyn Runtime>,
        pub db_manager: DBManagerFixture,
        pub usages: Arc<tokio::sync::Mutex<HashMap<StackID, Usage>>>,
        pub data_dir: TempDir,
    }

    #[async_trait]
    impl AsyncTestContext for RuntimeFixture {
        async fn setup() -> Self {
            install_wasm32_target();
            build_test_funcs();

            let db_manager = <DBManagerFixture as AsyncTestContext>::setup().await;
            let data_dir = TempDir::setup();

            let config = RuntimeConfig {
                cache_path: data_dir.get_rand_sub_dir(Some("runtime-cache")),
                include_function_logs: true,
            };

            let (runtime, mut notifications) =
                start(db_manager.db_manager.clone(), config).await.unwrap();

            let usages = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

            tokio::spawn({
                let usages = usages.clone();
                async move {
                    loop {
                        if let Some(n) = notifications.recv().await {
                            match n {
                                Notification::ReportUsage(stack_id, usage) => {
                                    let mut map = usages.lock().await;
                                    if let Entry::Vacant(e) = map.entry(stack_id) {
                                        e.insert(usage);
                                    } else {
                                        *map.get_mut(&stack_id).unwrap() += usage;
                                    }
                                }
                            }
                        }
                    }
                }
            });

            RuntimeFixture {
                runtime,
                db_manager,
                usages,
                data_dir,
            }
        }
        async fn teardown(self) {
            self.runtime.stop().await.unwrap();
            AsyncTestContext::teardown(self.db_manager).await;
            self.data_dir.teardown();
        }
    }

    pub struct RuntimeFixtureWithoutDB {
        pub runtime: Box<dyn Runtime>,
        pub usages: Arc<tokio::sync::Mutex<HashMap<StackID, Usage>>>,
        pub data_dir: TempDir,
    }

    #[async_trait]
    impl AsyncTestContext for RuntimeFixtureWithoutDB {
        async fn setup() -> Self {
            install_wasm32_target();
            build_test_funcs();

            let db_manager = mock_db::EmptyDBManager;
            let data_dir = TempDir::setup();

            let config = RuntimeConfig {
                cache_path: data_dir.get_rand_sub_dir(Some("runtime-cache")),
                include_function_logs: true,
            };

            let (runtime, mut notifications) = start(Box::new(db_manager), config).await.unwrap();

            let usages = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

            tokio::spawn({
                let usages = usages.clone();
                async move {
                    loop {
                        if let Some(n) = notifications.recv().await {
                            match n {
                                Notification::ReportUsage(stack_id, usage) => {
                                    let mut map = usages.lock().await;
                                    if let Entry::Vacant(e) = map.entry(stack_id) {
                                        e.insert(usage);
                                    } else {
                                        *map.get_mut(&stack_id).unwrap() += usage;
                                    }
                                }
                            }
                        }
                    }
                }
            });

            RuntimeFixtureWithoutDB {
                runtime,
                usages,
                data_dir,
            }
        }
        async fn teardown(self) {
            self.runtime.stop().await.unwrap();
            self.data_dir.teardown();
        }
    }
}

pub fn create_project<'a>(
    name: &'a str,
    functions: &'a [&'a str],
    memory_limit: &Option<byte_unit::Byte>,
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

pub async fn create_and_add_projects<'a>(
    definitions: Vec<(&'a str, &'a [&'a str], Option<byte_unit::Byte>)>,
    runtime: &dyn Runtime,
) -> Result<Vec<Project<'a>>> {
    let mut projects = vec![];

    for (name, funcs, mem_limit) in definitions.into_iter() {
        projects.push(create_project(name, funcs, &mem_limit));
    }

    let functions = read_wasm_functions(&projects).await?;
    let function_defs = functions.clone().into_values().into_iter().collect();
    runtime.add_functions(function_defs).await?;

    Ok(projects)
}

pub fn make_request<'a>(
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

mod mock_db {
    #![allow(unused)]
    use async_trait::async_trait;
    use mu_db::error::Result;
    use mu_db::{Blob, DbClient, DbManager, Key, Scan, TableName};
    use mu_stack::StackID;
    use tikv_client::Value;

    #[derive(Clone)]
    pub struct EmptyDBManager;

    #[derive(Debug, Clone)]
    pub struct EmptyDBClient;

    #[async_trait]
    impl DbClient for EmptyDBClient {
        async fn update_stack_tables(
            &self,
            stack_id: StackID,
            table_list: Vec<TableName>,
        ) -> Result<()> {
            Ok(())
        }

        async fn put(&self, key: Key, value: Value, is_atomic: bool) -> Result<()> {
            Ok(())
        }

        async fn get(&self, key: Key) -> Result<Option<Value>> {
            Ok(None)
        }

        async fn delete(&self, key: Key, is_atomic: bool) -> Result<()> {
            Ok(())
        }

        async fn delete_by_prefix(
            &self,
            stack_id: StackID,
            table_name: TableName,
            prefix_inner_key: Blob,
        ) -> Result<()> {
            Ok(())
        }

        async fn clear_table(&self, stack_id: StackID, table_name: TableName) -> Result<()> {
            Ok(())
        }

        async fn scan(&self, scan: Scan, limit: u32) -> Result<Vec<(Key, Value)>> {
            Ok(vec![])
        }

        async fn scan_keys(&self, scan: Scan, limit: u32) -> Result<Vec<Key>> {
            Ok(vec![])
        }

        async fn table_list(
            &self,
            stack_id: StackID,
            table_name_prefix: Option<TableName>,
        ) -> Result<Vec<TableName>> {
            Ok(vec![])
        }

        async fn stack_id_list(&self) -> Result<Vec<StackID>> {
            Ok(vec![])
        }

        async fn batch_delete(&self, keys: Vec<Key>) -> Result<()> {
            Ok(())
        }

        async fn batch_get(&self, keys: Vec<Key>) -> Result<Vec<(Key, Value)>> {
            Ok(vec![])
        }

        async fn batch_put(&self, pairs: Vec<(Key, Value)>, is_atomic: bool) -> Result<()> {
            Ok(())
        }

        async fn batch_scan(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<(Key, Value)>> {
            Ok(vec![])
        }

        async fn batch_scan_keys(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<Key>> {
            Ok(vec![])
        }

        async fn compare_and_swap(
            &self,
            key: Key,
            previous_value: Option<Value>,
            new_value: Value,
        ) -> Result<(Option<Value>, bool)> {
            Ok((None, false))
        }
    }

    #[async_trait]
    impl DbManager for EmptyDBManager {
        async fn make_client(&self) -> anyhow::Result<Box<dyn DbClient>> {
            Ok(Box::new(EmptyDBClient))
        }

        async fn stop_embedded_cluster(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
