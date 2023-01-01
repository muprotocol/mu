use std::{
    env,
    net::IpAddr,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::Child,
};

use super::error::{Error::EmbeddingTikvErr, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use mailbox_processor::callback::CallbackMailboxProcessor;
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::network::gossip::{KnownNodeConfig, NodeAddress};

#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Assets;

async fn check_and_extract_embedded_executable(name: &str) -> Result<PathBuf> {
    let mut temp_address = env::temp_dir();
    temp_address.push(name);

    if temp_address.exists() {
        return Ok(temp_address);
    }

    let tool = <Assets as RustEmbed>::get(name)
        .ok_or(EmbeddingTikvErr("Failed to get embedded asset".into()))?;

    let tool_bytes = tool.data;

    let temp_address_ref: &Path = temp_address.as_ref();

    let mut file = File::create(temp_address_ref)
        .await
        .map_err(|_| EmbeddingTikvErr("Failed to create temp file".into()))?;

    file.write_all(&tool_bytes)
        .await
        .map_err(|_| EmbeddingTikvErr("Failed to write embedded resource to temp file".into()))?;

    file.flush()
        .await
        .map_err(|_| EmbeddingTikvErr("Failed to flush temp file".into()))?;

    let mut perms = file
        .metadata()
        .await
        .map_err(|_| EmbeddingTikvErr("Failed to get temp file metadata".into()))?
        .permissions();

    perms.set_mode(0o744);

    file.set_permissions(perms)
        .await
        .map_err(|_| EmbeddingTikvErr("Failed to set executable permission on temp file".into()))?;

    Ok(temp_address)
}

// TODO: support hostname (also in gossip as well)
#[derive(Deserialize)]
pub struct IpAndPort {
    address: IpAddr,
    port: u16,
}

impl ToString for IpAndPort {
    fn to_string(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

#[derive(Deserialize)]
pub struct PdConfig {
    data_dir: String,
    peer_url: IpAndPort,
    pub client_url: IpAndPort,
    log_file: Option<String>,
}

#[derive(Deserialize)]
pub struct NodeConfig {
    cluster_url: IpAndPort,
    data_dir: String,
    log_file: Option<String>,
}

#[derive(Deserialize)]
pub struct TikvRunnerConfig {
    pub pd: PdConfig,
    pub node: NodeConfig,
}

#[async_trait]
#[clonable]
pub trait TikvRunner: Clone + Send + Sync {
    async fn stop(&self) -> Result<()>;
}

struct TikvRunnerArgs {
    pd_args: Vec<String>,
    tikv_args: Vec<String>,
}

enum Node<'a> {
    Known(&'a KnownNodeConfig),
    Node(&'a NodeAddress),
}

fn generate_pd_name(node: Node<'_>) -> String {
    const PD_PREFIX: &str = "pd_node_";
    match node {
        Node::Known(n) => format!("{PD_PREFIX}{}_{}", n.ip, n.gossip_port),
        Node::Node(n) => format!("{PD_PREFIX}{}_{}", n.address, n.port),
    }
}

fn generate_arguments(
    node_address: NodeAddress,
    known_node_config: Vec<KnownNodeConfig>,
    config: TikvRunnerConfig,
) -> TikvRunnerArgs {
    let mut initial_cluster = known_node_config
        .into_iter()
        .map(|node| {
            let name = generate_pd_name(Node::Known(&node));
            format!("{name}={}:{}", node.ip, node.pd_port)
        })
        .collect::<Vec<String>>();

    let pd_name = generate_pd_name(Node::Node(&node_address));

    initial_cluster.push(format!(
        "{pd_name}={}:{}",
        node_address.address, config.pd.peer_url.port
    ));

    let initial_cluster = initial_cluster.join(", ");

    let mut pd_args = vec![
        format!("--name={pd_name}"),
        format!("--data-dir={}", config.pd.data_dir),
        format!(
            "--client-urls=\"{}:{}\"",
            config.pd.client_url.address, config.pd.client_url.port
        ),
        format!(
            "--peer-urls=\"{}:{}\"",
            config.pd.peer_url.address, config.pd.peer_url.port
        ),
        format!("--initial-cluster=\"{initial_cluster}\""),
    ];

    if let Some(log_file) = config.pd.log_file {
        pd_args.push(format!("--log-file={log_file}"))
    }

    let mut tikv_args = vec![
        format!(
            "--pd-endpoints=\"{}:{}\"",
            config.pd.client_url.address, config.pd.client_url.port
        ),
        format!(
            "--addr=\"{}:{}\"",
            config.node.cluster_url.address, config.node.cluster_url.port
        ),
        format!("--data-dir={}", config.node.data_dir),
    ];

    if let Some(log_file) = config.node.log_file {
        tikv_args.push(format!("--log-file={log_file}"))
    }

    TikvRunnerArgs { pd_args, tikv_args }
}

enum Message {
    Stop,
}

#[derive(Clone)]
struct TikvRunnerImpl {
    mailbox: CallbackMailboxProcessor<Message>,
}

pub async fn start(
    node_address: NodeAddress,
    known_node_config: Vec<KnownNodeConfig>,
    config: TikvRunnerConfig,
) -> Result<Box<dyn TikvRunner>> {
    let tikv_version = env!("TIKV_VERSION");
    let pd_exe = check_and_extract_embedded_executable(&format!("pd-server-{tikv_version}"))
        .await
        .map_err(|_| EmbeddingTikvErr("Failed to create pd-exe".into()))?;
    let tikv_exe = check_and_extract_embedded_executable(&format!("tikv-server-{tikv_version}"))
        .await
        .map_err(|_| EmbeddingTikvErr("Failed to create tikv-exe".into()))?;

    let args = generate_arguments(node_address, known_node_config, config);

    let pd_process = std::process::Command::new(pd_exe)
        .args(args.pd_args)
        .spawn()
        .map_err(|_| EmbeddingTikvErr("Failed to spawn process pd!!".into()))?;

    let tikv_process = std::process::Command::new(tikv_exe)
        .args(args.tikv_args)
        .spawn()
        .map_err(|_| EmbeddingTikvErr("Failed to spawn process tikv!!".into()))?;

    let mailbox = CallbackMailboxProcessor::start(
        step,
        TikvRunnerState {
            pd_process,
            tikv_process,
        },
        10000,
    );

    let res = TikvRunnerImpl { mailbox };

    Ok(Box::new(res))
}

#[async_trait]
impl TikvRunner for TikvRunnerImpl {
    async fn stop(&self) -> Result<()> {
        self.mailbox
            .post(Message::Stop)
            .await
            .map_err(|e| EmbeddingTikvErr(format!("{e}")))
    }
}

struct TikvRunnerState {
    pd_process: Child,
    tikv_process: Child,
}

async fn step(
    _mb: CallbackMailboxProcessor<Message>,
    msg: Message,
    mut state: TikvRunnerState,
) -> TikvRunnerState {
    match msg {
        Message::Stop => {
            // TODO: consider unwraps
            state.pd_process.kill().unwrap();
            state.tikv_process.kill().unwrap();
        }
    }
    state
}
