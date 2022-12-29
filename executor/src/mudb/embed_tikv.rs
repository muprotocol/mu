use std::{
    env,
    net::IpAddr,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::Child,
};

use anyhow::{Context, Result};
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

    let tool = <Assets as RustEmbed>::get(name).context("Failed to get embedded asset")?;
    let tool_bytes = tool.data;

    let temp_address_ref: &Path = temp_address.as_ref();

    let mut file = File::create(temp_address_ref)
        .await
        .context("Failed to create temp file")?;

    file.write_all(&tool_bytes)
        .await
        .context("Failed to write embedded resource to temp file")?;

    file.flush().await.context("Failed to flush temp file")?;

    let mut perms = file
        .metadata()
        .await
        .context("Failed to get temp file metadata")?
        .permissions();

    perms.set_mode(0o744);

    file.set_permissions(perms)
        .await
        .context("Failed to set executable permission on temp file")?;

    Ok(temp_address)
}

// TODO: support hostname (also in gossip as well)
#[derive(Deserialize)]
pub struct IpAndPort {
    address: IpAddr,
    port: u16,
}

#[derive(Deserialize)]
pub struct PdConfig {
    data_dir: String,
    peer_url: IpAndPort,
    client_url: IpAndPort,
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
    pd: PdConfig,
    node: NodeConfig,
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

fn generate_pd_name(node: &KnownNodeConfig) -> String {
    const PD_PREFIX: &str = "pd_node_";
    format!("{PD_PREFIX}{}_{}", node.address, node.port)
}

fn generate_arguments(
    node_address: NodeAddress,
    known_node_config: Vec<KnownNodeConfig>,
    config: TikvRunnerConfig,
) -> TikvRunnerArgs {
    let mut initial_cluster = known_node_config
        .into_iter()
        .map(|node| {
            let name = generate_pd_name(&node);
            format!("{name}={}:{}", node.ip, node.pd_port)
        })
        .collect::<Vec<String>>();

    let pd_name = generate_pd_name(&node_address);

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

    if let (Some(log_file)) = config.pd.log_file {
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

    if let (Some(log_file)) = config.node.log_file {
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
        .context("Failed to create pd-exe")?;
    let tikv_exe = check_and_extract_embedded_executable(&format!("tikv-server-{tikv_version}"))
        .await
        .context("Failed to create tikv-exe")?;

    let args = generate_arguments(node_address, known_node_config, config);

    let pd_process = std::process::Command::new(pd_exe)
        .args(args.pd_args)
        .spawn()
        .context("Failed to spawn process pd!!")?;

    let tikv_process = std::process::Command::new(tikv_exe)
        .args(args.tikv_args)
        .spawn()
        .context("Failed to spawn process tikv!!")?;

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
        self.mailbox.post(Message::Stop).await.map_err(Into::into)
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
            state.pd_process.kill();
            state.tikv_process.kill();
        }
    }
    state
}
