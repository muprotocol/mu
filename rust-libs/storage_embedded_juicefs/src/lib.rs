use std::process::Stdio;
use std::{env, os::unix::prelude::PermissionsExt, path::PathBuf, process, vec};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use base64::Engine;
use dyn_clonable::clonable;
use log::error;
use mailbox_processor::callback::CallbackMailboxProcessor;
use mu_common::serde_support::TcpPortAddress;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::{fs::File, io::AsyncWriteExt};

const ACCESS_KEY: &str = "admin";
const BUCKET_NAME: &str = "mu-default";

#[derive(Deserialize, Clone, Debug)]
pub struct AuthConfig {
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub security_token: Option<String>,
    pub session_token: Option<String>,
    pub profile: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Region {
    pub region: String,
    pub endpoint: String,
}

#[derive(Clone, Deserialize, Debug)]
pub struct LiveStorageConfig {
    pub auth_config: AuthConfig,
    pub region: Region,
    pub bucket_name: String,
}

#[async_trait]
#[clonable]
pub trait JuicefsRunner: Clone + Send + Sync {
    async fn stop(&self) -> Result<()>;
}

enum Message {
    Stop,
}

struct JuicefsRunnerState {
    gateway_process: process::Child,
}

#[derive(Clone)]
struct JuicefsRunnerImpl {
    mailbox: CallbackMailboxProcessor<Message>,
}

#[async_trait]
impl JuicefsRunner for JuicefsRunnerImpl {
    async fn stop(&self) -> Result<()> {
        self.mailbox.post(Message::Stop).await?;
        // do we need to stop this ? and clone it too?
        // based on the comment on mailbox.stop it seems like we dont need to stop it.
        self.mailbox.clone().stop().await;
        Ok(())
    }
}

#[derive(RustEmbed)]
#[folder = "assets"]
pub struct Assets;

// TODO: move this in with db_embedded_tikv's version somewhere
async fn check_and_extract_embedded_executable(name: &str) -> Result<PathBuf> {
    let mut temp_address = env::temp_dir();
    temp_address.push(name);

    // TODO check checksum instead existing.
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

#[derive(Deserialize)]
pub struct StorageInfo {
    pub endpoint: TcpPortAddress,
}

#[derive(Deserialize)]
pub struct InternalStorageConfig {
    pub metadata_tikv_endpoints: Vec<TcpPortAddress>,
    pub object_storage_tikv_endpoints: Vec<TcpPortAddress>,
    pub storage: StorageInfo,
}

struct Args {
    format_args: Vec<String>,
    gateway_args: Vec<String>,
}

fn generate_arguments(config: &InternalStorageConfig) -> Args {
    fn tikv_endpoints(ports: &[TcpPortAddress]) -> String {
        ports
            .iter()
            .map(|ip| ip.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    let metadata_endpoints = tikv_endpoints(config.metadata_tikv_endpoints.as_ref());

    let format_args = vec![
        "format".to_owned(),
        "--storage".to_owned(),
        "tikv".to_owned(),
        "--bucket".to_owned(),
        tikv_endpoints(config.object_storage_tikv_endpoints.as_ref()),
        format!("tikv://{metadata_endpoints}"),
        BUCKET_NAME.to_string(),
    ];

    let gateway_args = vec![
        "gateway".to_owned(),
        format!("tikv://{metadata_endpoints}"),
        config.storage.endpoint.to_string(),
    ];

    Args {
        format_args,
        gateway_args,
    }
}

async fn step(
    _mb: CallbackMailboxProcessor<Message>,
    msg: Message,
    mut state: JuicefsRunnerState,
) -> JuicefsRunnerState {
    match msg {
        Message::Stop => {
            if let Err(f) = signal::kill(
                Pid::from_raw(state.gateway_process.id().try_into().unwrap()),
                Signal::SIGINT,
            ) {
                error!("failed to kill juicefs gateway process due to: {f:?}")
            }

            if let Err(e) = state.gateway_process.wait() {
                error!("failed to wait for juicefs gateway process to exit due to: {e:?}")
            }
        }
    }
    state
}

pub async fn start(
    config: &InternalStorageConfig,
) -> Result<(Box<dyn JuicefsRunner>, LiveStorageConfig)> {
    let tag_name = env!("TAG_NAME");

    let juicefs_exe = check_and_extract_embedded_executable(&format!("juicefs-{tag_name}"))
        .await
        .context("Failed to create juicefs executable")?;

    let args = generate_arguments(config);

    let format_exit_output = std::process::Command::new(&juicefs_exe)
        .args(args.format_args)
        .output()
        .context("Failed to run juicefs format")?;
    if !format_exit_output.status.success() {
        bail!(
            "Failed to format JuiceFS storage:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(format_exit_output.stdout.as_ref()),
            String::from_utf8_lossy(format_exit_output.stderr.as_ref())
        );
    }

    let secret_key = base64::engine::general_purpose::STANDARD.encode(rand::random::<[u8; 30]>());

    let gateway_process = std::process::Command::new(juicefs_exe)
        .args(args.gateway_args)
        .env("MINIO_ROOT_USER", ACCESS_KEY)
        .env("MINIO_ROOT_PASSWORD", secret_key.clone())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn process juicefs gateway")?;

    let mailbox =
        CallbackMailboxProcessor::start(step, JuicefsRunnerState { gateway_process }, 10000);

    let live_storage_config = LiveStorageConfig {
        auth_config: AuthConfig {
            access_key: Some(ACCESS_KEY.to_string()),
            secret_key: Some(secret_key),
            security_token: None,
            session_token: None,
            profile: None,
        },
        region: Region {
            region: "us-east-1".to_owned(),
            endpoint: format!("http://{}", config.storage.endpoint),
        },
        bucket_name: BUCKET_NAME.to_string(),
    };

    Ok((Box::new(JuicefsRunnerImpl { mailbox }), live_storage_config))
}
