pub mod gateway;
pub mod infrastructure;
pub mod mu_stack;
pub mod mudb;
pub mod network;
pub mod runtime;
pub mod util;

use std::{process, time::SystemTime};

use anyhow::{bail, Context, Result};
use gateway::GatewayManager;
use log::*;
use mailbox_processor::NotificationChannel;
use runtime::Runtime;
use tokio::{select, sync::mpsc};
use tokio_util::sync::CancellationToken;

use infrastructure::{config, log_setup};
use network::{
    connection_manager::{self, ConnectionManager, ConnectionManagerNotification},
    gossip::{GossipNotification, KnownNodeConfig},
};

use crate::network::gossip::{self, Gossip, NodeAddress};

pub async fn run() -> Result<()> {
    // TODO handle failures in components

    let cancellation_token = CancellationToken::new();
    let cancellation_token_clone = cancellation_token.clone();

    ctrlc::set_handler(move || cancellation_token_clone.cancel())
        .context("Failed to initialize Ctrl+C handler")?;

    let (
        connection_manager_config,
        gossip_config,
        mut known_nodes_config,
        gateway_manager_config,
        log_config,
    ) = config::initialize_config()?;

    let my_node = NodeAddress {
        address: connection_manager_config.listen_address,
        port: connection_manager_config.listen_port,
        generation: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    };

    let is_seed = known_nodes_config
        .iter()
        .any(|n| is_same_node_as_me(n, &my_node));
    known_nodes_config.retain(|n| !is_same_node_as_me(n, &my_node));

    log_setup::setup(log_config)?;

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

    let (gossip_notification_channel, mut gossip_notification_receiver) =
        NotificationChannel::new();

    let mut known_nodes = vec![];

    info!("Establishing connection to seeds");

    for node in known_nodes_config {
        match connection_manager.connect(node.address, node.port).await {
            Ok(connection_id) => known_nodes.push((
                NodeAddress {
                    address: node.address,
                    port: node.port,
                    generation: 0,
                },
                connection_id,
            )),

            Err(f) => warn!(
                "Failed to connect to seed {}:{}, will ignore this seed. Error is {f}",
                node.address, node.port
            ),
        }

        if cancellation_token.is_cancelled() {
            process::exit(0);
        }
    }

    if known_nodes.is_empty() && !is_seed {
        bail!("Failed to connect to any seeds and this node is not a seed, aborting");
    }

    let gossip = gossip::start(
        my_node,
        gossip_config,
        known_nodes,
        gossip_notification_channel,
    )
    .context("Failed to start gossip")?;

    let function_provider = runtime::providers::DefaultFunctionProvider::new();
    let runtime = runtime::start(Box::new(function_provider));

    // TODO: no notification channel for now, requests are sent straight to runtime
    let gateway_manager = gateway::start(gateway_manager_config, runtime.clone())
        .await
        .context("Failed to start gateway manager")?;

    // TODO remove this
    deploy_prototype_stack(runtime.clone(), gateway_manager.clone()).await;

    // TODO: create a `Module`/`Subsystem`/`NotificationSource` trait to batch modules with their notification receivers?
    glue_modules(
        cancellation_token,
        connection_manager.as_ref(),
        connection_manager_notification_receiver,
        gossip.as_ref(),
        &mut gossip_notification_receiver,
    )
    .await;

    // Stop gateway manager first. This waits for rocket to shut down, essentially
    // running all requests to completion or cancelling them safely before shutting
    // the rest of the system down.
    gateway_manager
        .stop()
        .await
        .context("Failed to stop gateway manager")?;

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

fn is_same_node_as_me(node: &KnownNodeConfig, me: &NodeAddress) -> bool {
    node.port == me.port && (node.address == me.address || node.address.is_loopback())
}

// TODO
async fn deploy_prototype_stack(
    runtime: Box<dyn Runtime>,
    gateway_manager: Box<dyn GatewayManager>,
) {
    let yaml = std::fs::read_to_string("./prototype/stack.yaml").unwrap();
    let stack = serde_yaml::from_str::<mu_stack::Stack>(yaml.as_str()).unwrap();
    let id = mu_stack::StackID("00001111-2222-3333-4444-555566667777".parse().unwrap());
    mu_stack::deploy::deploy(id, stack, runtime, gateway_manager)
        .await
        .unwrap();
    warn!("Deployed prototype stack with ID {id}");
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
