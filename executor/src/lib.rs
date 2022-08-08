mod config;
mod connection_manager;
pub mod gossip;
pub mod runtime;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use env_logger::Env;

use log::*;

use connection_manager::ConnectionID;

use crate::connection_manager::ConnectionManager;

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

    let connection_manager_config = connection_manager::ConnectionManagerConfig {
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

    connection_manager.set_callbacks(CB(
        // connection_manager,
        connection_manager_config.listen_port == 12012,
    ));

    connection_manager
        .start(connection_manager_config)
        .await
        .context("Failed to start connection manager")?;

    // do something!
    let port = config
        .get_string("connection_manager.listen_port")?
        .parse::<i32>()?;
    if port == 12012 {
        loop {}
    } else {
        let id = loop {
            match connection_manager
                .connect("127.0.0.1".parse()?, 12012)
                .await
            {
                Ok(x) => break x,
                Err(f) => error!("Failed to connect due to {}, will retry", f),
            }
        };

        let data = "Hello!".into();
        connection_manager.send_datagram(id, data).await?;

        let data = "Hello!".into();
        let rep = connection_manager.send_req_rep(id, data).await?;
        info!("Received reply: {}", String::from_utf8_lossy(&rep));

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        connection_manager.stop().await?;
    }

    info!("Goodbye!");

    Ok(())
}

#[derive(Clone)]
struct CB(bool);

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
