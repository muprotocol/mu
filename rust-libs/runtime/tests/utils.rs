use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::Result;
use mu_db::{
    DbManager, DbManagerImpl, IpAndPort, NodeAddress, PdConfig, TikvConfig, TikvRunnerConfig,
};
use musdk_common::Header;
use rstest::fixture;

use async_trait::async_trait;
use mu_runtime::{
    start, AssemblyDefinition, AssemblyProvider, Notification, Runtime, RuntimeConfig, Usage,
};
use mu_stack::{AssemblyID, AssemblyRuntime, FunctionID, StackID};

// Add test project names (directory name) in this array to build them when testing
const TEST_PROJECTS: &'static [&'static str] = &[
    "hello-wasm",
    "calc-func",
    "multi-body",
    "unclean-termination",
];

#[derive(Default)]
pub struct MapAssemblyProvider {
    inner: HashMap<AssemblyID, AssemblyDefinition>,
}

#[async_trait]
impl AssemblyProvider for MapAssemblyProvider {
    fn get(&self, id: &AssemblyID) -> Option<&AssemblyDefinition> {
        self.inner.get(id)
    }

    fn add_function(&mut self, function: AssemblyDefinition) {
        self.inner.insert(function.id.clone(), function);
    }

    fn remove_function(&mut self, id: &AssemblyID) {
        self.inner.remove(id);
    }

    fn get_function_names(&self, _stack_id: &StackID) -> Vec<String> {
        unimplemented!("Not needed")
    }
}

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
            AssemblyDefinition::new(
                project.id.clone(),
                source.into(),
                AssemblyRuntime::Wasi1_0,
                [],
                project.memory_limit,
            ),
        );
    }

    Ok(results)
}

pub mod fixture {
    use super::*;

    macro_rules! block_on {
        ($async_expr:expr) => {{
            tokio::task::block_in_place(|| {
                let handle = tokio::runtime::Handle::current();
                handle.block_on($async_expr)
            })
        }};
    }

    pub struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    impl TempDir {
        pub fn new() -> Self {
            Self(std::env::temp_dir().join(Self::rand_dir_name()))
        }

        pub fn get_rand_subdir(&self, prefix: Option<&str>) -> PathBuf {
            let name = format!("{}{}", prefix.unwrap_or(""), Self::rand_dir_name());
            self.0.join(name)
        }

        fn rand_dir_name() -> String {
            let rand: [u8; 5] = rand::random();
            rand.into_iter()
                .fold(String::new(), |a, i| format!("{a}{i}"))
        }
    }

    #[fixture]
    #[once]
    fn temp_dir_fixture() -> TempDir {
        TempDir::new()
    }

    #[fixture]
    #[once]
    pub fn install_wasm32_wasi_target_fixture() {
        Command::new("rustup")
            .args(["target", "add", "wasm32-wasi"])
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
    }

    #[fixture]
    #[once]
    pub fn build_test_funcs_fixture() {
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
        }
    }

    pub struct DBManagerWrapper {
        data_dir: PathBuf,
        db_manager: DbManagerImpl,
    }

    impl DBManagerWrapper {
        pub fn get_db_manager(&self) -> Box<dyn DbManager> {
            Box::new(self.db_manager.clone())
        }
    }

    impl Drop for DBManagerWrapper {
        fn drop(&mut self) {
            block_on!(self.db_manager.stop_embedded_cluster()).unwrap()
        }
    }

    #[fixture]
    #[once]
    pub fn mudb_fixture(temp_dir_fixture: &TempDir) -> DBManagerWrapper {
        let data_dir = temp_dir_fixture.get_rand_subdir(Some("mudb"));
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
                data_dir: data_dir.join("pd_data_dir").display().to_string(),
                log_file: Some(data_dir.join("pd_log").display().to_string()),
            },
            node: TikvConfig {
                cluster_url: IpAndPort {
                    address: localhost,
                    port: 20163,
                },
                data_dir: data_dir.join("tikv_data_dir").display().to_string(),
                log_file: Some(data_dir.join("tikv_log").display().to_string()),
            },
        };

        DBManagerWrapper {
            data_dir,
            db_manager: block_on!(DbManagerImpl::new_with_embedded_cluster(
                node_address,
                vec![],
                tikv_config,
            ))
            .unwrap(),
        }
    }

    #[fixture]
    pub fn runtime_fixture<'a>(
        _install_wasm32_wasi_target_fixture: (),
        _build_test_funcs_fixture: (),
        temp_dir_fixture: &TempDir,
        mudb_fixture: &'a DBManagerWrapper,
    ) -> RuntimeFixture {
        let assembly_provider = Box::new(MapAssemblyProvider::default());

        let config = RuntimeConfig {
            cache_path: temp_dir_fixture.get_rand_subdir(Some("runtime-cache")),
            include_function_logs: true,
        };

        let (runtime, mut notifications) = block_on!(start(
            assembly_provider,
            mudb_fixture.get_db_manager(),
            config
        ))
        .unwrap();

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

        (runtime, mudb_fixture.get_db_manager(), usages)
    }

    pub type RuntimeFixture = (
        Box<dyn Runtime>,
        Box<dyn DbManager>,
        Arc<tokio::sync::Mutex<HashMap<StackID, Usage>>>,
    );
}

//pub async fn create_runtime<'a>(
//    projects: &'a [Project<'a>],
//) -> (
//    Box<dyn Runtime>,
//    Box<dyn DbManager>,
//    Arc<Mutex<HashMap<StackID, Usage>>>,
//) {
//    let config = RuntimeConfig {
//        cache_path: get_temp_dir(),
//        include_function_logs: true,
//    };
//
//    let (functions, provider) = create_map_function_provider(projects).await.unwrap();
//    let (runtime, mut notifications) = start(Box::new(provider), config).await.unwrap();
//
//    runtime
//        .add_functions(functions.clone().into_values().into_iter().collect())
//        .await
//        .unwrap();
//
//    let usages = Arc::new(Mutex::new(HashMap::new()));
//
//    tokio::spawn({
//        let usages = usages.clone();
//        async move {
//            loop {
//                match notifications.recv().await {
//                    None => continue,
//                    Some(n) => match n {
//                        Notification::ReportUsage(stack_id, usage) => {
//                            let mut map = usages.lock().await;
//                            if let Entry::Vacant(e) = map.entry(stack_id) {
//                                e.insert(usage);
//                            } else {
//                                *map.get_mut(&stack_id).unwrap() += usage;
//                            }
//                        }
//                    },
//                }
//            }
//        }
//    });
//
//    (runtime, Box::new(db_manager), usages)
//}

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
    runtime: &Box<dyn Runtime>,
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
