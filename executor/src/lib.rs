pub mod gossip;
mod infrastructure;
mod network;
pub mod runtime;

use std::process;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use log::*;
use tokio_util::sync::CancellationToken;

use infrastructure::{config, log_setup};
use network::connection_manager::{self, ConnectionID, ConnectionManager, ConnectionManagerConfig};

pub async fn run() -> Result<()> {
    let cancellation_token = CancellationToken::new();
    let cancellation_token_clone = cancellation_token.clone();

    ctrlc::set_handler(move || cancellation_token_clone.cancel())
        .context("Failed to initialize Ctrl+C handler")?;

    let config = config::initialize_config(vec![
        ("log_level", "warn"),
        ("connection_manager.listen_ip", "0.0.0.0"),
        ("connection_manager.listen_port", "12012"),
    ])?;

    log_setup::setup(&config)?;

    info!("Initializing Mu...");

    let connection_manager_config = ConnectionManagerConfig {
        listen_address: config
            .get_string("connection_manager.listen_address")?
            .parse()
            .context("Failed to parse listen address")?,
        listen_port: config
            .get_string("connection_manager.listen_port")?
            .parse()
            .context("Failed to parse listen port")?,
        max_request_response_size: 8 * 1024,
    };

    let mut connection_manager = connection_manager::new();

    connection_manager.set_callbacks(CB());

    if cancellation_token.is_cancelled() {
        process::exit(0);
    }

    connection_manager
        .start(connection_manager_config)
        .await
        .context("Failed to start connection manager")?;

    if cancellation_token.is_cancelled() {
        process::exit(0);
    }

    // Connect to and query seed nodes
    // Start gossip

    // TODO handle failures in components

    cancellation_token.cancelled().await;
    info!("Received SIGINT, stopping");

    connection_manager
        .stop()
        .await
        .context("Failed to stop connection manager")?;

    info!("Goodbye!");

    Ok(())
}

#[derive(Clone)]
struct CB();

#[async_trait]
impl connection_manager::ConnectionManagerCallbacks for CB {
    async fn new_connection_available(&self, id: ConnectionID) {
        info!("New connection available: {}", id);
    }

    async fn connection_closed(&self, id: ConnectionID) {
        info!("Connection closed: {}", id);
    }

    async fn datagram_received(&self, id: ConnectionID, data: Bytes) {
        debug!(
            "Datagram received from {}: {}",
            id,
            String::from_utf8_lossy(&data)
        );
    }

    async fn req_rep_received(&self, id: ConnectionID, data: Bytes) -> Bytes {
        debug!(
            "Req-rep received from {}: {}",
            id,
            String::from_utf8_lossy(&data)
        );

        data
    }
}
