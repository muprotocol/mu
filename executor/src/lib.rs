pub mod gossip;
mod infrastructure;
mod network;
pub mod runtime;

use std::process;

use anyhow::{Context, Result};
use log::*;
use tokio::{select, sync::mpsc};
use tokio_mailbox_processor::NotificationChannel;
use tokio_util::sync::CancellationToken;

use infrastructure::{config, log_setup};
use network::connection_manager::{self, ConnectionManager, ConnectionManagerNotification};

pub async fn run() -> Result<()> {
    let cancellation_token = CancellationToken::new();
    let cancellation_token_clone = cancellation_token.clone();

    ctrlc::set_handler(move || cancellation_token_clone.cancel())
        .context("Failed to initialize Ctrl+C handler")?;

    let (config, connection_manager_config) = config::initialize_config()?;

    let port = connection_manager_config.listen_port; // TODO

    log_setup::setup(&config)?;

    info!("Initializing Mu...");

    let (notif_channel, mut connection_manager_notification_receiver) = NotificationChannel::new();

    let connection_manager = connection_manager::start(connection_manager_config, notif_channel)
        .await
        .context("Failed to start connection manager")?;

    if cancellation_token.is_cancelled() {
        process::exit(0);
    }

    // Connect to and query seed nodes
    // Start gossip

    // TODO handle failures in components

    if port != 12012 {
        let id = connection_manager
            .connect("127.0.0.1".parse().unwrap(), 12012)
            .await?;
        connection_manager
            .send_datagram(id, "Hello!".into())
            .await?;
        let resp = connection_manager.send_req_rep(id, "Ooooh!".into()).await?;
        println!("{resp:?}");
    }

    loop {
        select! {
            () = cancellation_token.cancelled() => {
                info!("Received SIGINT, stopping");
                break;
            }

            () = process_connection_manager_notifications(
                &connection_manager,
                &mut connection_manager_notification_receiver
            ) => ()
        }
    }

    cancellation_token.cancelled().await;

    connection_manager
        .stop()
        .await
        .context("Failed to stop connection manager")?;

    info!("Goodbye!");

    Ok(())
}

async fn process_connection_manager_notifications(
    connection_manager: &dyn ConnectionManager,
    rx: &mut mpsc::UnboundedReceiver<ConnectionManagerNotification>,
) {
    match rx.recv().await {
        None => (),
        Some(ConnectionManagerNotification::NewConnectionAvailable(id)) => {
            info!("New connection available: {}", id)
        }
        Some(ConnectionManagerNotification::ConnectionClosed(id)) => {
            info!("Connection closed: {}", id)
        }
        Some(ConnectionManagerNotification::DatagramReceived(id, bytes)) => debug!(
            "Datagram received from {}: {}",
            id,
            String::from_utf8_lossy(&bytes)
        ),
        Some(ConnectionManagerNotification::ReqRepReceived(id, req_id, bytes)) => {
            debug!(
                "Req-rep received from {}: {}",
                id,
                String::from_utf8_lossy(&bytes)
            );
            if let Err(f) = connection_manager.send_reply(id, req_id, bytes).await {
                error!("Failed to send reply: {}", f);
            }
        }
    }
}
