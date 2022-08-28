pub mod infrastructure;
pub mod mu_stack;
pub mod mudb;
pub mod network;
pub mod runtime;
pub mod util;

use std::{process, time::SystemTime};

use anyhow::{Context, Result};
use log::*;
use mailbox_processor::NotificationChannel;
use tokio::{select, sync::mpsc};
use tokio_util::sync::CancellationToken;

use infrastructure::{config, log_setup};
use network::{
    connection_manager::{self, ConnectionManager, ConnectionManagerNotification},
    gossip::GossipNotification,
};

use crate::network::gossip::{self, Gossip, KnownNodes, NodeAddress};

pub async fn run() -> Result<()> {
    let cancellation_token = CancellationToken::new();
    let cancellation_token_clone = cancellation_token.clone();

    ctrlc::set_handler(move || cancellation_token_clone.cancel())
        .context("Failed to initialize Ctrl+C handler")?;

    let (config, connection_manager_config, gossip_config) = config::initialize_config()?;

    let port = connection_manager_config.listen_port; // TODO

    log_setup::setup(&config)?;

    info!("Initializing Mu...");

    let (connection_manager_notification_channel, connection_manager_notification_receiver) =
        NotificationChannel::new();

    let connection_manager = connection_manager::start(
        connection_manager_config,
        connection_manager_notification_channel,
    )
    .context("Failed to start connection manager")?;

    if cancellation_token.is_cancelled() {
        process::exit(0);
    }

    // Connect to and query seed nodes
    // Start gossip

    // TODO handle failures in components

    let mut known_nodes: KnownNodes = vec![];

    if port != 12012 {
        let address = "127.0.0.1".parse().unwrap();
        let id = connection_manager.connect(address, 12012).await?;
        known_nodes.push((
            NodeAddress {
                address,
                port: 12012,
                generation: 0,
            },
            id,
        ));
    }

    let my_node = NodeAddress {
        address: "127.0.0.1".parse().unwrap(),
        port,
        generation: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    };

    let (gossip_notification_channel, mut gossip_notification_receiver) =
        NotificationChannel::new();

    let gossip = gossip::start(
        my_node,
        gossip_config,
        known_nodes,
        gossip_notification_channel,
    )
    .context("Failed to start gossip")?;

    // TODO: create a `Module`/`Subsystem`/`NotificationSource` trait to batch modules with their notification receivers?
    glue_modules(
        cancellation_token,
        connection_manager.as_ref(),
        connection_manager_notification_receiver,
        gossip.as_ref(),
        &mut gossip_notification_receiver,
    )
    .await;

    gossip.stop().await.context("Failed to stop gossip")?;

    // The glue loop shouldn't stop as soon as it receives a ctrl+C
    loop {
        match gossip_notification_receiver.recv().await {
            None => break,
            Some(notification) => {
                process_gossip_notification(
                    Some(notification),
                    connection_manager.as_ref(),
                    gossip.as_ref(),
                )
                .await
            }
        }
    }

    connection_manager
        .stop()
        .await
        .context("Failed to stop connection manager")?;

    info!("Goodbye!");

    Ok(())
}

async fn glue_modules(
    cancellation_token: CancellationToken,
    connection_manager: &dyn ConnectionManager,
    mut connection_manager_notification_receiver: mpsc::UnboundedReceiver<
        ConnectionManagerNotification,
    >,
    gossip: &dyn Gossip,
    gossip_notification_receiver: &mut mpsc::UnboundedReceiver<GossipNotification>,
) {
    let mut debug_timer = tokio::time::interval(std::time::Duration::from_secs(3));

    loop {
        select! {
            () = cancellation_token.cancelled() => {
                info!("Received SIGINT, stopping");
                break;
            }

            _ = debug_timer.tick() => {
                let nodes = gossip.get_nodes().await;
                match nodes {
                    Ok(peers) => warn!("Discovered nodes: {:?}", peers),
                    Err(f) => error!("Failed to get nodes: {}", f)
                }
            }

            notification = connection_manager_notification_receiver.recv() => {
                process_connection_manager_notification(notification, connection_manager, gossip).await;
            }

            notification = gossip_notification_receiver.recv() => {
                process_gossip_notification(notification, connection_manager, gossip).await;
            }
        }
    }
}

async fn process_connection_manager_notification(
    notification: Option<ConnectionManagerNotification>,
    connection_manager: &dyn ConnectionManager,
    gossip: &dyn Gossip,
) {
    match notification {
        None => (), // TODO
        Some(ConnectionManagerNotification::NewConnectionAvailable(id)) => {
            info!("New connection available: {}", id)
        }
        Some(ConnectionManagerNotification::ConnectionClosed(id)) => {
            info!("Connection closed: {}", id)
        }
        Some(ConnectionManagerNotification::DatagramReceived(id, bytes)) => {
            debug!(
                "Datagram received from {}: {}",
                id,
                String::from_utf8_lossy(&bytes)
            );

            gossip.receive_message(id, bytes);
        }
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

async fn process_gossip_notification(
    notification: Option<GossipNotification>,
    connection_manager: &dyn ConnectionManager,
    gossip: &dyn Gossip,
) {
    match notification {
        None => (), // TODO
        Some(GossipNotification::NodeDiscovered(node)) => {
            debug!("Node discovered: {node}");
        }
        Some(GossipNotification::NodeDied(node, cleanly)) => {
            debug!(
                "Node died {}: {node}",
                if cleanly { "cleanly" } else { "uncleanly" }
            );
        }
        Some(GossipNotification::Connect(req_id, address, port)) => {
            match connection_manager.connect(address, port).await {
                Ok(id) => gossip.connection_available(req_id, id),
                Err(f) => gossip.connection_failed(req_id, f),
            }
        }
        Some(GossipNotification::SendMessage(id, bytes)) => {
            connection_manager.send_datagram(id, bytes);
        }
        Some(GossipNotification::Disconnect(id)) => {
            connection_manager.disconnect(id).await.unwrap_or(());
        }
    }
}
