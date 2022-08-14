pub mod infrastructure;
pub mod network;
pub mod runtime;

use std::{process, time::SystemTime};

use anyhow::{Context, Result};
use log::*;
use tokio::{select, sync::mpsc};
use tokio_mailbox_processor::NotificationChannel;
use tokio_util::sync::CancellationToken;

use infrastructure::{config, log_setup};
use network::{
    connection_manager::{self, ConnectionManager, ConnectionManagerNotification},
    gossip::GossipNotification,
};

use crate::network::gossip::{self, Gossip, KnownNodes, Node};

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
    .await
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
            Node {
                address,
                port: 12012,
                generation: 0,
            },
            id,
        ));
    }

    let my_node = Node {
        address: "127.0.0.1".parse().unwrap(),
        port,
        generation: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    };

    let (gossip_notification_channel, gossip_notification_receiver) = NotificationChannel::new();

    let gossip = gossip::start(
        my_node,
        gossip_config,
        known_nodes,
        gossip_notification_channel,
    )
    .await
    .context("Failed to start gossip")?;

    // TODO: create a `Module`/`Subsystem`/`NotificationSource` trait to batch modules with their notification receivers?
    glue_modules(
        cancellation_token,
        &connection_manager,
        connection_manager_notification_receiver,
        &gossip,
        gossip_notification_receiver,
    )
    .await;

    gossip.stop().await.context("Failed to stop gossip")?;

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
    mut gossip_notification_receiver: mpsc::UnboundedReceiver<GossipNotification>,
) {
    let mut debug_timer = tokio::time::interval(std::time::Duration::from_secs(3));

    loop {
        select! {
            () = cancellation_token.cancelled() => {
                info!("Received SIGINT, stopping");
                break;
            }

            _ = debug_timer.tick() => {
                let peers = gossip.get_peers().await;
                match peers {
                    Ok(peers) => warn!("Connected peers: {:?}", peers),
                    Err(f) => error!("Failed to get peers: {}", f)
                }
            }

            notification = connection_manager_notification_receiver.recv() => {
                process_connection_manager_notification(notification, connection_manager, gossip).await;
            }

            notification = gossip_notification_receiver.recv() => {
                process_gossip_notification(notification, connection_manager).await;
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

            gossip.receive_message(id, bytes).await;
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
) {
    match notification {
        None => (), // TODO
        Some(GossipNotification::PeerStatusUpdated(peer, status)) => {
            debug!("Peer {} now has status {:?}", peer.node(), status);
        }
        Some(GossipNotification::SendMessage(id, bytes)) => {
            connection_manager.send_datagram(id, bytes);
        }
    }
}
