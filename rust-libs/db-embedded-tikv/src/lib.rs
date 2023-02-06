//! This module takes a lot of time (~11 secs on a Ryzen9 5900) to type check
//! due to the embedded resources. We moved it to a separate crate to improve
//! type check times when developing the DB module.

use std::{
    env,
    net::{IpAddr, Ipv4Addr},
    os::unix::prelude::PermissionsExt,
    path::PathBuf,
    process,
};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::{error, warn};
use mailbox_processor::callback::CallbackMailboxProcessor;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::{fs::File, io::AsyncWriteExt};

#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Assets;

async fn check_and_extract_embedded_executable(name: &str) -> Result<PathBuf> {
    let mut temp_address = env::temp_dir();
    temp_address.push(name);

    // TODO: remove if and let create temp files every time.
    // also let this to be separate for TikvRunner
    // in test module concurrent test need to create temp files once.
    // otherwise they get race condition of creating temp files.
    let file = if temp_address.exists() {
        File::open(temp_address.as_path())
            .await
            .context("Failed to open temp file")?
    } else {
        let mut file = File::create(temp_address.as_path())
            .await
            .context("Failed to create temp file")?;

        let tool = <Assets as RustEmbed>::get(name).context("Failed to get embedded asset")?;
        let tool_bytes = tool.data;

        file.write_all(&tool_bytes)
            .await
            .context("Failed to write embedded resource to temp file")?;

        file.flush().await.context("Failed to flush temp file")?;

        file
    };

    let mut perms = file
        .metadata()
        .await
        .context("Failed to get temp file metadata")?
        .permissions();

    perms.set_mode(0o500);

    file.set_permissions(perms)
        .await
        .context("Failed to set executable permission on temp file")?;

    Ok(temp_address)
}

// TODO: support hostname (also in gossip as well)
/// # IpAndPort
#[derive(Deserialize, Clone, PartialEq, Eq)]
pub struct IpAndPort {
    pub address: IpAddr,
    pub port: u16,
}

impl From<IpAndPort> for String {
    fn from(value: IpAndPort) -> Self {
        format!("{}:{}", value.address, value.port)
    }
}

impl TryFrom<&str> for IpAndPort {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        let x: Vec<&str> = value.split(':').collect();
        if x.len() != 2 {
            bail!("Can't parse, expected string in this format: ip_addr:port");
        } else {
            Ok(IpAndPort {
                address: x[0].parse()?,
                port: x[1].parse()?,
            })
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct PdConfig {
    pub data_dir: PathBuf,
    pub peer_url: IpAndPort,
    pub client_url: IpAndPort,
    pub log_file: Option<PathBuf>,
}

fn unspecified_to_localhost(x: &IpAndPort) -> IpAndPort {
    IpAndPort {
        address: match x.address {
            xp if xp.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            xp => xp,
        },
        port: x.port,
    }
}

impl PdConfig {
    pub fn advertise_client_url(&self) -> IpAndPort {
        unspecified_to_localhost(&self.client_url)
    }

    pub fn advertise_peer_url(&self) -> IpAndPort {
        unspecified_to_localhost(&self.peer_url)
    }
}

#[derive(Deserialize, Clone)]
pub struct TikvConfig {
    pub cluster_url: IpAndPort,
    pub data_dir: PathBuf,
    pub log_file: Option<PathBuf>,
}

impl TikvConfig {
    pub fn advertise_cluster_url(&self) -> IpAndPort {
        unspecified_to_localhost(&self.cluster_url)
    }
}

#[derive(Deserialize, Clone)]
pub struct TikvRunnerConfig {
    pub pd: PdConfig,
    pub node: TikvConfig,
}

#[async_trait]
#[clonable]
pub trait TikvRunner: Clone + Send + Sync {
    async fn stop(&self) -> Result<()>;
}

struct TikvRunnerArgs {
    pub pd_args: Vec<String>,
    pub tikv_args: Vec<String>,
}

pub struct RemoteNode {
    pub address: IpAddr,
    pub gossip_port: u16,
    pub pd_port: u16,
}

enum Node<'a> {
    Known(&'a RemoteNode),
    Address(&'a IpAndPort),
}

fn generate_pd_name(node: Node<'_>) -> String {
    const PD_PREFIX: &str = "pd_node_";
    match node {
        Node::Known(n) => format!("{PD_PREFIX}{}_{}", n.address, n.gossip_port),
        Node::Address(n) => format!("{PD_PREFIX}{}_{}", n.address, n.port),
    }
}

fn generate_arguments(
    node_address: IpAndPort,
    known_node_config: Vec<RemoteNode>,
    config: TikvRunnerConfig,
) -> TikvRunnerArgs {
    let warn = |x| {
        warn!(
            "{x} listen address is 0.0.0.0, \
            which will listen on all IP's, even those connected to the internet. \
            This is very dangerous, continue only if you are completely certain \
            you know what you're doing."
        )
    };
    if config.pd.peer_url.address.is_unspecified() {
        warn("PD peer");
    }
    if config.pd.client_url.address.is_unspecified() {
        warn("PD client");
    }
    if config.node.cluster_url.address.is_unspecified() {
        warn("TiKV");
    }

    let mut initial_cluster = known_node_config
        .into_iter()
        .map(|node| {
            let name = generate_pd_name(Node::Known(&node));
            format!("{name}=http://{}:{}", node.address, node.pd_port)
        })
        .collect::<Vec<String>>();

    let pd_name = generate_pd_name(Node::Address(&node_address));

    initial_cluster.insert(
        0,
        format!(
            "{pd_name}=http://{}:{}",
            config.pd.advertise_peer_url().address,
            config.pd.advertise_peer_url().port
        ),
    );

    let initial_cluster = initial_cluster.join(",");

    let mut pd_args = vec![
        format!("--name={pd_name}"),
        format!("--data-dir={}", config.pd.data_dir.display()),
        format!(
            "--client-urls=http://{}:{}",
            config.pd.client_url.address, config.pd.client_url.port
        ),
        format!(
            "--advertise-client-urls=http://{}:{}",
            config.pd.advertise_client_url().address,
            config.pd.advertise_client_url().port
        ),
        format!(
            "--peer-urls=http://{}:{}",
            config.pd.peer_url.address, config.pd.peer_url.port
        ),
        format!(
            "--advertise-peer-urls=http://{}:{}",
            config.pd.advertise_peer_url().address,
            config.pd.advertise_peer_url().port
        ),
        format!("--initial-cluster={initial_cluster}"),
    ];

    if let Some(log_file) = config.pd.log_file.as_ref() {
        pd_args.push(format!("--log-file={}", log_file.display()))
    }

    let mut tikv_args = vec![
        format!(
            "--pd-endpoints=http://{}:{}",
            config.pd.advertise_client_url().address,
            config.pd.advertise_client_url().port
        ),
        format!(
            "--addr={}:{}",
            config.node.cluster_url.address, config.node.cluster_url.port
        ),
        format!(
            "--advertise-addr={}:{}",
            config.node.advertise_cluster_url().address,
            config.node.advertise_cluster_url().port
        ),
        format!("--data-dir={}", config.node.data_dir.display()),
    ];

    if let Some(log_file) = config.node.log_file {
        tikv_args.push(format!("--log-file={}", log_file.display()))
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
    node_address: IpAndPort,
    known_node_config: Vec<RemoteNode>,
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
        .context("Failed to spawn process pd")?;

    let tikv_process = std::process::Command::new(tikv_exe)
        .args(args.tikv_args)
        .spawn()
        .context("Failed to spawn process tikv")?;

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
        self.mailbox.post(Message::Stop).await?;
        self.mailbox.clone().stop().await;
        Ok(())
    }
}

struct TikvRunnerState {
    pub pd_process: process::Child,
    pub tikv_process: process::Child,
}

async fn step(
    _mb: CallbackMailboxProcessor<Message>,
    msg: Message,
    mut state: TikvRunnerState,
) -> TikvRunnerState {
    match msg {
        Message::Stop => {
            if let Err(f) = signal::kill(
                Pid::from_raw(state.tikv_process.id().try_into().unwrap()),
                Signal::SIGINT,
            ) {
                error!("failed to kill tikv_process due to: {f:?}")
            }

            if let Err(e) = state.tikv_process.wait() {
                error!("failed to wait for tikv to exit {e:?}")
            }

            if let Err(f) = signal::kill(
                Pid::from_raw(state.pd_process.id().try_into().unwrap()),
                Signal::SIGINT,
            ) {
                error!("failed to kill pd_process due to: {f:?}")
            }

            if let Err(e) = state.pd_process.wait() {
                error!("failed to wait for pd to exit {e:?}")
            }
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
        let node_address = IpAndPort {
            address: local_host,
            port: 2800,
        };
        let known_node_conf = vec![
            RemoteNode {
                address: local_host,
                gossip_port: 2801,
                pd_port: 2381,
            },
            RemoteNode {
                address: local_host,
                gossip_port: 2802,
                pd_port: 2383,
            },
        ];
        let tikv_runner_conf = TikvRunnerConfig {
            pd: PdConfig {
                peer_url: IpAndPort {
                    address: local_host,
                    port: 2380,
                },
                client_url: IpAndPort {
                    address: local_host,
                    port: 2379,
                },
                data_dir: PathBuf::from("./pd_test_dir"),
                log_file: None,
            },
            node: TikvConfig {
                cluster_url: IpAndPort {
                    address: local_host,
                    port: 20160,
                },
                data_dir: PathBuf::from("./tikv_test_dir"),
                log_file: None,
            },
        };

        let res = generate_arguments(node_address, known_node_conf, tikv_runner_conf);

        assert_eq!(res.pd_args[0], "--name=pd_node_127.0.0.1_2800");
        assert_eq!(res.pd_args[1], "--data-dir=./pd_test_dir");
        assert_eq!(res.pd_args[2], "--client-urls=http://127.0.0.1:2379");
        assert_eq!(
            res.pd_args[3],
            "--advertise-client-urls=http://127.0.0.1:2379"
        );
        assert_eq!(res.pd_args[4], "--peer-urls=http://127.0.0.1:2380");
        assert_eq!(
            res.pd_args[5],
            "--advertise-peer-urls=http://127.0.0.1:2380"
        );
        assert_eq!(
            res.pd_args[6],
            "--initial-cluster=\
                pd_node_127.0.0.1_2800=http://127.0.0.1:2380,\
                pd_node_127.0.0.1_2801=http://127.0.0.1:2381,\
                pd_node_127.0.0.1_2802=http://127.0.0.1:2383"
        );

        assert_eq!(res.tikv_args[0], "--pd-endpoints=http://127.0.0.1:2379");
        assert_eq!(res.tikv_args[1], "--addr=127.0.0.1:20160");
        assert_eq!(res.tikv_args[2], "--advertise-addr=127.0.0.1:20160");
        assert_eq!(res.tikv_args[3], "--data-dir=./tikv_test_dir");
    }
}
