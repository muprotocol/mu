mod config;
mod connection_manager;
pub mod gossip;
pub mod runtime;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use env_logger::Env;

use log::info;

use connection_manager::ConnectionID;

pub async fn run() -> Result<()> {
    let config = config::initialize_config(vec![
        ("log_level", "warn"),
        ("connection_manager.listen_ip", "0.0.0.0"),
        ("connection_manager.listen_port", "12012"),
    ])?;

    env_logger::Builder::from_env(
        Env::default().default_filter_or(config.get_string("log_level")?),
    )
    .init();

    info!("Initializing Mu...");

    let connection_manager = connection_manager::start(
        config
            .get_string("connection_manager.listen_address")?
            .parse()
            .context("Failed to parse listen address")?,
        config
            .get_string("connection_manager.listen_port")?
            .parse()
            .context("Failed to parse listen port")?,
        Box::new(CB {}),
    )
    .await
    .context("Failed to start connection manager")?;

    // do something!
    let port = config
        .get_string("connection_manager.listen_port")?
        .parse::<i32>()?;
    if port == 12012 {
        loop {}
    } else {
        let id = connection_manager
            .connect("127.0.0.1".parse()?, 12012)
            .await
            .context("Failed to connect")?;

        let data = "Hello!".into();
        connection_manager.send_datagram(id, data).await?;

        let data = "Hello!".into();
        let rep = connection_manager.send_req_rep(id, data).await?;
        info!("Received reply: {}", String::from_utf8_lossy(&rep));

        connection_manager.stop().await?;
    }

    info!("Goodbye!");

    Ok(())
}

struct CB {}

#[async_trait]
impl connection_manager::ConnectionManagerCallbacks for CB {
    async fn new_connection_available(&self, id: ConnectionID) {
        info!("New connection available: {}", id);
    }

    async fn connection_closed(&self, id: ConnectionID) {
        info!("Connection closed: {}", id);
    }

    async fn datagram_received(&self, id: ConnectionID, data: Bytes) {
        info!(
            "Datagram received from {}: {}",
            id,
            String::from_utf8_lossy(&data)
        );
    }

    async fn req_rep_received(&self, id: ConnectionID, data: Bytes) -> Bytes {
        info!(
            "Req/Rep received from {}: {}",
            id,
            String::from_utf8_lossy(&data)
        );
        data
    }
}
