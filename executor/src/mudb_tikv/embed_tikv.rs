use super::{
    error::{Error::EmbedTikvErr, Result},
    types::IpAndPort,
};
use async_trait::async_trait;
use dyn_clonable::clonable;
use mailbox_processor::callback::CallbackMailboxProcessor;
use rust_embed::RustEmbed;
use serde::Deserialize;
use std::{
    env,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::Child,
};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::network::{gossip::KnownNodeConfig, NodeAddress};

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
        .ok_or(EmbedTikvErr("Failed to get embedded asset".into()))?;

    let tool_bytes = tool.data;

    let temp_address_ref: &Path = temp_address.as_ref();

    let mut file = File::create(temp_address_ref)
        .await
        .map_err(|_| EmbedTikvErr("Failed to create temp file".into()))?;

    file.write_all(&tool_bytes)
        .await
        .map_err(|_| EmbedTikvErr("Failed to write embedded resource to temp file".into()))?;

    file.flush()
        .await
        .map_err(|_| EmbedTikvErr("Failed to flush temp file".into()))?;

    let mut perms = file
        .metadata()
        .await
        .map_err(|_| EmbedTikvErr("Failed to get temp file metadata".into()))?
        .permissions();

    perms.set_mode(0o744);

    file.set_permissions(perms)
        .await
        .map_err(|_| EmbedTikvErr("Failed to set executable permission on temp file".into()))?;

    Ok(temp_address)
}

#[derive(Deserialize, Clone)]
pub struct PdConfig {
    pub data_dir: String,
    // TODO: address should be same with node address (NodeAddress::address) as below has been explained,
    // * so should be remove I think.
    // * https://tikv.org/docs/dev/deploy/configure/pd-command-line/#--peer-urls
    // * also I suggest to use a random port because it's for internal communication and
    // * that's not important to user
    pub peer_url: IpAndPort,
    pub client_url: IpAndPort,
    pub log_file: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct TikvConfig {
    // TODO: address should be same with node address (NodeAddress::address) as below has been explained,
    // * so should be remove I think.
    // * https://tikv.org/docs/dev/deploy/configure/pd-command-line/#--peer-urls
    // * also I suggest to use a random port because it's for internal communication and
    // * that's not important to user
    pub cluster_url: IpAndPort,
    // TODO: I suggest to make it optional and provide default data_dir
    pub data_dir: String,
    pub log_file: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct TikvRunnerConfig {
    pub pd: PdConfig,
    pub tikv: TikvConfig,
}

#[async_trait]
#[clonable]
pub trait TikvRunner: Clone + Send + Sync {
    async fn stop(&self) -> Result<()>;
    // async fn is_tikv_ready(&self) -> Result<bool>;
}

struct TikvRunnerArgs {
    pub pd_args: Vec<String>,
    pub tikv_args: Vec<String>,
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
            format!("{name}=http://{}:{}", node.ip, node.pd_port)
        })
        .collect::<Vec<String>>();

    let pd_name = generate_pd_name(Node::Node(&node_address));

    initial_cluster.insert(
        0,
        format!(
            "{pd_name}=http://{}:{}",
            node_address.address, config.pd.peer_url.port
        ),
    );

    let initial_cluster = initial_cluster.join(",");

    let mut pd_args = vec![
        format!("--name={pd_name}"),
        format!("--data-dir={}", config.pd.data_dir),
        format!(
            "--client-urls=http://{}:{}",
            config.pd.client_url.address, config.pd.client_url.port
        ),
        format!(
            "--peer-urls=http://{}:{}",
            config.pd.peer_url.address, config.pd.peer_url.port
        ),
        format!("--initial-cluster={initial_cluster}"),
    ];

    if let Some(log_file) = config.pd.log_file {
        pd_args.push(format!("--log-file={log_file}"))
    }

    let mut tikv_args = vec![
        format!(
            "--pd-endpoints=http://{}:{}",
            config.pd.client_url.address, config.pd.client_url.port
        ),
        format!(
            "--addr={}:{}",
            config.tikv.cluster_url.address, config.tikv.cluster_url.port
        ),
        format!("--data-dir={}", config.tikv.data_dir),
    ];

    if let Some(log_file) = config.tikv.log_file {
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
        .map_err(|_| EmbedTikvErr("Failed to create pd-exe".into()))?;
    let tikv_exe = check_and_extract_embedded_executable(&format!("tikv-server-{tikv_version}"))
        .await
        .map_err(|_| EmbedTikvErr("Failed to create tikv-exe".into()))?;

    let args = generate_arguments(node_address, known_node_config, config);

    let pd_process = std::process::Command::new(pd_exe)
        .args(args.pd_args)
        .spawn()
        .map_err(|_| EmbedTikvErr("Failed to spawn process pd!!".into()))?;

    let tikv_process = std::process::Command::new(tikv_exe)
        .args(args.tikv_args)
        .spawn()
        .map_err(|_| EmbedTikvErr("Failed to spawn process tikv!!".into()))?;

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
            .map_err(|e| EmbedTikvErr(format!("{e}")))
    }
}

struct TikvRunnerState {
    pub pd_process: Child,
    pub tikv_process: Child,
}

async fn step(
    _mb: CallbackMailboxProcessor<Message>,
    msg: Message,
    mut state: TikvRunnerState,
) -> TikvRunnerState {
    match msg {
        Message::Stop => {
            // TODO: consider handle results
            let _ = state.pd_process.kill();
            let _ = state.tikv_process.kill();
        }
    }
    state
}

#[cfg(test)]
mod test {
    use std::net::IpAddr;

    use super::*;

    #[tokio::test]
    async fn generate_arguments_pd_args_and_tikv_args() {
        let local_host: IpAddr = "127.0.0.1".parse().unwrap();
        let node_address = NodeAddress {
            address: local_host.clone(),
            port: 2800,
            generation: 1,
        };
        let known_node_conf = vec![
            KnownNodeConfig {
                ip: local_host.clone(),
                gossip_port: 2801,
                pd_port: 2381,
            },
            KnownNodeConfig {
                ip: local_host.clone(),
                gossip_port: 2802,
                pd_port: 2383,
            },
        ];
        let tikv_runner_conf = TikvRunnerConfig {
            pd: PdConfig {
                peer_url: IpAndPort {
                    address: local_host.clone(),
                    port: 2380,
                },
                client_url: IpAndPort {
                    address: local_host.clone(),
                    port: 2379,
                },
                data_dir: "./pd_test_dir".into(),
                log_file: None,
            },
            tikv: TikvConfig {
                cluster_url: IpAndPort {
                    address: local_host.clone(),
                    port: 20160,
                },
                data_dir: "./tikv_test_dir".into(),
                log_file: None,
            },
        };

        let res = generate_arguments(node_address, known_node_conf, tikv_runner_conf);
        assert_eq!(res.pd_args[0], "--name=pd_node_127.0.0.1_2800");
        assert_eq!(res.pd_args[1], "--data-dir=./pd_test_dir");
        assert_eq!(res.pd_args[2], "--client-urls=http://127.0.0.1:2379");
        assert_eq!(res.pd_args[3], "--peer-urls=http://127.0.0.1:2380");
        assert_eq!(
            res.pd_args[4],
            "--initial-cluster=\
                pd_node_127.0.0.1_2801=http://127.0.0.1:2381,\
                pd_node_127.0.0.1_2802=http://127.0.0.1:2383,\
                pd_node_127.0.0.1_2800=http://127.0.0.1:2380"
        );

        assert_eq!(res.tikv_args[0], "--pd-endpoints=http://127.0.0.1:2379");
        assert_eq!(res.tikv_args[1], "--addr=127.0.0.1:20160");
        assert_eq!(res.tikv_args[2], "--data-dir=./tikv_test_dir");
    }
}
