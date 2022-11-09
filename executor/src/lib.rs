pub mod gateway;
pub mod infrastructure;
pub mod mudb;
pub mod network;
pub mod runtime;
pub mod stack;
pub mod util;

use std::{
    process,
    time::{Duration, SystemTime},
};

use anyhow::{bail, Context, Result};
use log::*;
use mailbox_processor::NotificationChannel;
use stack::blockchain_monitor::{BlockchainMonitor, BlockchainMonitorNotification};
use tokio::{select, sync::mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    infrastructure::{config, log_setup},
    network::{
        connection_manager::{self, ConnectionManager, ConnectionManagerNotification},
        gossip::{self, Gossip, GossipNotification, KnownNodeConfig, NodeAddress},
    },
    stack::{
        blockchain_monitor,
        scheduler::{self, Scheduler, SchedulerNotification},
    },
};
use mudb::service::DatabaseManager;

pub async fn run() -> Result<()> {
    // TODO handle failures in components

    let cancellation_token = CancellationToken::new();
    let cancellation_token_clone = cancellation_token.clone();

    ctrlc::set_handler(move || cancellation_token_clone.cancel())
        .context("Failed to initialize Ctrl+C handler")?;

    let config::SystemConfig(
        connection_manager_config,
        gossip_config,
        mut known_nodes_config,
        gateway_manager_config,
        log_config,
        runtime_config,
        scheduler_config,
        blockchain_monitor_config,
    ) = config::initialize_config()?;

    let my_node = NodeAddress {
        address: connection_manager_config.listen_address,
        port: connection_manager_config.listen_port,
        generation: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    };
    let my_hash = my_node.get_hash();

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

    let usage_aggregator = stack::usage_aggregator::start();

    let function_provider = runtime::providers::DefaultFunctionProvider::new();
    let database_manager = DatabaseManager::new().await?;
    let runtime = runtime::start(
        Box::new(function_provider),
        runtime_config,
        database_manager.clone(),
    )
    .await
    .context("Failed to initiate runtime")?;

    // TODO: no notification channel for now, requests are sent straight to runtime
    let gateway_manager = gateway::start(gateway_manager_config, runtime.clone(), usage_aggregator)
        .await
        .context("Failed to start gateway manager")?;

    // TODO: fetch stacks from blockchain before starting scheduler
    let (scheduler_notification_channel, mut scheduler_notification_receiver) =
        NotificationChannel::new();
    let scheduler = scheduler::start(
        scheduler_config,
        my_hash,
        gossip.get_nodes().await?.into_iter().map(|n| n.0).collect(),
        vec![],
        scheduler_notification_channel,
        runtime.clone(),
        gateway_manager.clone(),
        database_manager.clone(),
    );

    let (blockchain_monitor, mut blockchain_monitor_notification_receiver) =
        blockchain_monitor::start(blockchain_monitor_config)
            .await
            .context("Failed to start blockchain monitor")?;

    // TODO: create a `Module`/`Subsystem`/`NotificationSource` trait to batch modules with their notification receivers?
    let scheduler_clone = scheduler.clone();
    let glue_task = tokio::spawn(async move {
        glue_modules(
            cancellation_token,
            connection_manager.as_ref(),
            connection_manager_notification_receiver,
            gossip.as_ref(),
            &mut gossip_notification_receiver,
            scheduler.as_ref(),
            &mut scheduler_notification_receiver,
            blockchain_monitor.as_ref(),
            &mut blockchain_monitor_notification_receiver,
        )
        .await;

        blockchain_monitor
            .stop()
            .await
            .context("Failed to stop blockchain monitor")?;

        scheduler.stop().await.context("Failed to stop scheduler")?;

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
                        scheduler.as_ref(),
                    )
                    .await
                }
            }
        }

        connection_manager
            .stop()
            .await
            .context("Failed to stop connection manager")?;

        Result::<()>::Ok(())
    });

    // TODO make the wait configurable
    {
        info!("Waiting 4 seconds for node discovery to complete");
        tokio::time::sleep(Duration::from_secs(4)).await;

        info!("Will start to schedule stacks now");
        scheduler_clone.ready_to_schedule_stacks().await?;
    }

    glue_task.await??;

    info!("Goodbye!");

    Ok(())
}

fn is_same_node_as_me(node: &KnownNodeConfig, me: &NodeAddress) -> bool {
    node.port == me.port && (node.address == me.address || node.address.is_loopback())
}

#[allow(clippy::too_many_arguments)]
async fn glue_modules(
    cancellation_token: CancellationToken,
    connection_manager: &dyn ConnectionManager,
    mut connection_manager_notification_receiver: mpsc::UnboundedReceiver<
        ConnectionManagerNotification,
    >,
    gossip: &dyn Gossip,
    gossip_notification_receiver: &mut mpsc::UnboundedReceiver<GossipNotification>,
    scheduler: &dyn Scheduler,
    scheduler_notification_receiver: &mut mpsc::UnboundedReceiver<SchedulerNotification>,
    _blockchain_monitor: &dyn BlockchainMonitor,
    blockchain_monitor_notification_receiver: &mut mpsc::UnboundedReceiver<
        BlockchainMonitorNotification,
    >,
) {
    let mut debug_timer = tokio::time::interval(Duration::from_secs(3));

    loop {
        select! {
            () = cancellation_token.cancelled() => {
                info!("Received SIGINT, stopping");
                break;
            }

            _ = debug_timer.tick() => {
                let nodes = gossip.get_nodes().await;
                match nodes {
                    Ok(peers) => {
                        warn!(
                            "Discovered nodes: {:?}",
                            peers.iter().map(|n| format!("{}:{}", n.1.address, n.1.port)).collect::<Vec<_>>()
                        );
                    },
                    Err(f) => error!("Failed to get nodes: {}", f),
                }
            }

            notification = connection_manager_notification_receiver.recv() => {
                process_connection_manager_notification(notification, connection_manager, gossip).await;
            }

            notification = gossip_notification_receiver.recv() => {
                process_gossip_notification(notification, connection_manager, gossip, scheduler).await;
            }

            notification = scheduler_notification_receiver.recv() => {
                process_scheduler_notification(notification, gossip).await;
            }

            notification = blockchain_monitor_notification_receiver.recv() => {
                process_blockchain_monitor_notification(notification, scheduler).await;
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
    scheduler: &dyn Scheduler,
) {
    match notification {
        None => (), // TODO
        Some(GossipNotification::NodeDiscovered(node)) => {
            debug!("Node discovered: {node}");
            scheduler.node_discovered(node.get_hash()).await.unwrap(); // TODO: unwrap
        }
        Some(GossipNotification::NodeDied(node, cleanly)) => {
            debug!(
                "Node died {}: {node}",
                if cleanly { "cleanly" } else { "uncleanly" }
            );
            scheduler.node_died(node.get_hash()).await.unwrap(); // TODO: unwrap
        }
        Some(GossipNotification::NodeDeployedStacks(node, stack_ids)) => {
            debug!("Node deployed stacks: {node} <- {stack_ids:?}");
            scheduler
                .node_deployed_stacks(node.get_hash(), stack_ids)
                .await
                .unwrap(); // TODO: unwrap
        }
        Some(GossipNotification::NodeUndeployedStacks(node, stack_ids)) => {
            debug!("Node undeployed stack: {node} <- {stack_ids:?}");
            scheduler
                .node_undeployed_stacks(node.get_hash(), stack_ids)
                .await
                .unwrap(); // TODO: unwrap
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

async fn process_scheduler_notification(
    notification: Option<SchedulerNotification>,
    gossip: &dyn Gossip,
) {
    match notification {
        None => (), // TODO
        Some(SchedulerNotification::StackDeployed(id)) => {
            debug!("Deployed stack {id}");
            gossip.stack_deployed_locally(id).await.unwrap(); // TODO: unwrap
        }
        Some(SchedulerNotification::StackUndeployed(id)) => {
            debug!("Undeployed stack {id}");
            gossip.stack_undeployed_locally(id).await.unwrap(); // TODO: unwrap
        }
        Some(SchedulerNotification::FailedToDeployStack(id)) => {
            debug!("Failed to deploy stack {id}");
        }
    }
}

async fn process_blockchain_monitor_notification(
    notification: Option<BlockchainMonitorNotification>,
    scheduler: &dyn Scheduler,
) {
    match notification {
        None => (), // TODO
        Some(BlockchainMonitorNotification::StacksAvailable(stacks)) => {
            debug!("Stacks available: {stacks:?}");
            for stack in stacks {
                scheduler
                    .stack_available(stack.id(), stack.stack)
                    .await
                    .unwrap();
            }
        }
    }
}
