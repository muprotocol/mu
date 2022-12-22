use std::{
    env,
    net::IpAddr,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use mailbox_processor::callback::CallbackMailboxProcessor;
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::network::gossip::NodeAddress;

#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Assets;

async fn extract_embedded_executable(name: &str) -> Result<PathBuf> {
    let tool = <Assets as RustEmbed>::get(name).context("Failed to get embedded asset")?;
    let tool_bytes = tool.data;

    let mut temp_address = env::temp_dir();
    temp_address.push(name);
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

// impl PdArgs<'_> {
//     fn into_arg_string(self) -> Vec<String> {
//         let client_urls = self
//             .client_urls
//             .into_iter()
//             .map(|uri| uri.to_string())
//             .collect::<Vec<String>>()
//             .join(", ");

//         let peer_urls = self
//             .peer_urls
//             .into_iter()
//             .map(|uri| uri.to_string())
//             .collect::<Vec<String>>()
//             .join(", ");

//         let initial_cluster = self
//             .initial_cluster
//             .into_iter()
//             .map(|(name, uri)| format!("{name}={uri}"))
//             .collect::<Vec<String>>()
//             .join(", ");

//         let mut args = vec![
//             format!("--name={}", self.name),
//             format!("--data-dir={}", self.data_dir),
//             format!("--client-urls=\"{client_urls}\""),
//             format!("--peer-urls=\"{peer_urls}\"",),
//             format!("--initial-cluster=\"{initial_cluster}\""),
//         ];

//         match self.log_file {
//             Some(path) => args.push(format!("--log-file={}", path)),
//             None => (),
//         };

//         args
//     }
// }

// impl TikvArgs<'_> {
//     fn into_arg_string(self) -> Vec<String> {
//         let pd_endpoints = self
//             .pd_endpoints
//             .into_iter()
//             .map(|uri| uri.to_string())
//             .collect::<Vec<String>>()
//             .join(", ");

//         let mut args = vec![
//             format!("--pd-endpoints=\"{pd_endpoints}\""),
//             format!("--addr=\"{}\"", self.address),
//             format!("--data-dir={}", self.data_dir),
//         ];

//         match &self.log_file {
//             Some(path) => args.push(format!("--log-file={}", path)),
//             None => (),
//         };

//         args
//     }
// }

#[async_trait]
#[clonable]
pub trait TikvRunner: Clone + Send + Sync {
    async fn stop(&self);
}

// // TODO: Rename
enum Message {
    Stop,
}

// // TODO: Rename
struct TikvRunnerImpl {
    mailbox: CallbackMailboxProcessor<Message>,
}

pub fn start(
    node_address: NodeAddress,
    gossip_seeds: &[NodeAddress],
    config: TikvRunnerConfig,
) -> Box<dyn TikvRunner> {
    todo!()
    //     let mailbox = CallbacjMailboxProcessor::start(
    //         step,
    //         TikvEmbedState {
    //             pd_executable: None,
    //             tikv_executable: None,
    //         },
    //         10000,
    //     );

    //     let res = TikvEmbedImpl { mailbox };

    //     Box::new(res)
}

//#[async_trait]
// impl TikvEmbed for TikvEmbedImpl {
//     async fn start_pd(&self, args: PdArgs) -> Result<()> {
//         self.mailbox
//             .post(TikvMessage::StartPd(args))
//             .await
//             .map_error(Into::into)
//     }

//     async fn start_tikv(&self, args: TikvArgs) -> Result<()> {
//         self.mailbox
//             .post(TikvMessage::StartTikv(args))
//             .await
//             .map_error(Into::into)
//     }
// }

// // TODO: Rename
// TODO: store processes
// struct TikvEmbedState {
//     pd_executable: Option<Path>,
//     tikv_executbale: Option<Path>,
// }

// TODO: remove ***ALL*** unwraps
// async fn step(
//     _mb: CallbackMailboxProcessor<TikvMessage>,
//     msg: TikvMessage,
//     mut state: TikvEmbedState,
// ) -> TikvEmbedState {
//     match msg {
//         TikvMessage::StartPd(args) => match state.pd_executable {
//             None => {
//                 let exe = extract_embedded_executable("pd_server");
//                 std::process::Command::new(exe).args(args).spawn().unwrap();
//                 state.pd_executable = Some(exe);
//             }
//             Some(exe) => std::process::Command::new(exe).args(args).spawn().unwrap(),
//         },

//         TikvMessage::StartTikv(args) => match state.tikv_executable {
//             None => {
//                 let exe = extract_embedded_executable("tikv_server");
//                 std::process::Command::new(exe).args(args).spawn().unwrap();
//                 state.tikv_executable = Some(exe);
//             }
//             Some(exe) => std::process::Command::new(exe).args(args).spawn().unwrap(),
//         },
//     }

//     state
// }
