pub mod api;
pub mod infrastructure;
pub mod network;
pub mod request_routing;
pub mod stack;

use std::{process, sync::Arc, time::SystemTime};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use log::*;
use mailbox_processor::NotificationChannel;
use mu_runtime::Runtime;
use network::rpc_handler::{self, RpcHandler, RpcRequestHandler};
use stack::{
    blockchain_monitor::{BlockchainMonitor, BlockchainMonitorNotification},
    request_signer_cache::RequestSignerCache,
    usage_aggregator::{Usage, UsageAggregator},
};
use tokio::{
    select,
    sync::{mpsc, RwLock},
};
use tokio_util::sync::CancellationToken;

use crate::{
    infrastructure::{config, log_setup},
    network::{
        connection_manager::{self, ConnectionManager, ConnectionManagerNotification},
        gossip::{self, Gossip, GossipNotification, KnownNodeConfig},
        NodeAddress,
    },
    stack::{
        blockchain_monitor, request_signer_cache,
        scheduler::{self, Scheduler, SchedulerNotification},
    },
};

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
        db_config,
        gateway_manager_config,
        log_config,
        runtime_config,
        scheduler_config,
        blockchain_monitor_config,
    ) = config::initialize_config()?;

    let stabilization_wait_time = *gossip_config.network_stabilization_wait_time;

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

    for node in &known_nodes_config {
        match connection_manager
            .connect(node.address, node.gossip_port)
            .await
        {
            Ok(connection_id) => known_nodes.push((
                NodeAddress {
                    address: node.address,
                    port: node.gossip_port,
                    generation: 0,
                },
                connection_id,
            )),

            Err(f) => warn!(
                "Failed to connect to seed {}:{}, will ignore this seed. Error is {f}",
                node.address, node.gossip_port
            ),
        }

        if cancellation_token.is_cancelled() {
            process::exit(0);
        }
    }

    if known_nodes.is_empty() && !is_seed {
        bail!("Failed to connect to any seeds and this node is not a seed, aborting");
    }

    let usage_aggregator = stack::usage_aggregator::start();

    let (blockchain_monitor, mut blockchain_monitor_notification_receiver, region_id) =
        blockchain_monitor::start(blockchain_monitor_config, usage_aggregator.clone())
            .await
            .context("Failed to start blockchain monitor")?;

    let gossip = gossip::start(
        my_node.clone(),
        gossip_config,
        known_nodes,
        gossip_notification_channel,
        region_id,
    )
    .context("Failed to start gossip")?;

    let database_manager = mu_db::start(
        mu_db::NodeAddress {
            address: my_node.address,
            port: my_node.port,
        },
        known_nodes_config
            .iter()
            .map(|c| mu_db::RemoteNode {
                address: c.address,
                gossip_port: c.gossip_port,
                pd_port: c.pd_port,
            })
            .collect(),
        db_config,
    )
    .await?;

    let (runtime, mut runtime_notification_receiver) =
        mu_runtime::start(database_manager.clone(), runtime_config)
            .await
            .context("Failed to initiate runtime")?;

    let rpc_handler = rpc_handler::new(
        connection_manager.clone(),
        RpcRequestHandlerImpl {
            runtime: runtime.clone(),
        },
    );

    let request_signer_cache = request_signer_cache::start();

    let connection_manager_clone = connection_manager.clone();
    let gossip_clone = gossip.clone();
    let rpc_handler_clone = rpc_handler.clone();
    let runtime_clone = runtime.clone();

    let scheduler_ref = Arc::new(RwLock::new(None));
    let scheduler_ref_clone = scheduler_ref.clone();
    let (gateway_manager, mut gateway_notification_receiver) = mu_gateway::start(
        gateway_manager_config,
        api::service_factory(),
        Some(api::DependencyAccessor {
            request_signer_cache: request_signer_cache.clone(),
        }),
        move |f, r| {
            Box::pin(request_routing::route_request(
                f,
                r,
                connection_manager_clone.clone(),
                gossip_clone.clone(),
                scheduler_ref_clone.clone(),
                rpc_handler_clone.clone(),
                runtime_clone.clone(),
            ))
        },
    )
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

    *scheduler_ref.write().await = Some(scheduler.clone());

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
            rpc_handler.as_ref(),
            usage_aggregator.as_ref(),
            &mut gateway_notification_receiver,
            &mut runtime_notification_receiver,
            request_signer_cache.as_ref(),
        )
        .await;

        trace!("Stopping blockchain monitor");
        blockchain_monitor
            .stop()
            .await
            .context("Failed to stop blockchain monitor")?;

        trace!("Stopping scheduler");
        scheduler.stop().await.context("Failed to stop scheduler")?;

        trace!("Stopping runtime");
        runtime.stop().await.context("Failed to stop runtime")?;

        trace!("Stopping database manager");
        database_manager
            .stop_embedded_cluster()
            .await
            .context("Failed to stop runtime")?;

        // Stop gateway manager first. This waits for actix-web to shut down, essentially
        // running all requests to completion or cancelling them safely before shutting
        // the rest of the system down.
        trace!("Stopping gateway manager");
        gateway_manager
            .stop()
            .await
            .context("Failed to stop gateway manager")?;

        request_signer_cache.stop().await;

        trace!("Stopping gossip");
        gossip.stop().await.context("Failed to stop gossip")?;

        // The glue loop shouldn't stop as soon as it receives a ctrl+C
        trace!("Draining gossip notifications");
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

        trace!("Stopping connection manager");
        connection_manager
            .stop()
            .await
            .context("Failed to stop connection manager")?;

        Result::<()>::Ok(())
    });

    {
        info!(
            "Waiting {} seconds for gossip state to stabilize",
            stabilization_wait_time.as_secs()
        );
        tokio::time::sleep(stabilization_wait_time).await;

        info!("Will start to schedule stacks now");
        scheduler_clone.ready_to_schedule_stacks().await?;
    }

    glue_task.await??;

    info!("Goodbye!");

    Ok(())
}

fn is_same_node_as_me(node: &KnownNodeConfig, me: &NodeAddress) -> bool {
    node.gossip_port == me.port && (node.address == me.address || node.address.is_loopback())
}

#[derive(Clone)]
struct RpcRequestHandlerImpl {
    runtime: Box<dyn Runtime>,
}

#[async_trait]
impl RpcRequestHandler for RpcRequestHandlerImpl {
    async fn handle_request(&self, request: rpc_handler::RpcRequest) {
        let rpc_handler::RpcRequest::ExecuteFunctionRequest(function_id, request, send_response) =
            request;

        let helper = async move {
            let result = self
                .runtime
                .invoke_function(function_id, request)
                .await
                .context("Failed to invoke function")?;

            Ok(result)
        };

        send_response(helper.await).await;
    }
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
    rpc_handler: &dyn RpcHandler,
    usage_aggregator: &dyn UsageAggregator,
    gateway_notification_receiver: &mut mpsc::UnboundedReceiver<mu_gateway::Notification>,
    runtime_notification_receiver: &mut mpsc::UnboundedReceiver<mu_runtime::Notification>,
    request_signer_cache: &dyn RequestSignerCache,
) {
    loop {
        select! {
            () = cancellation_token.cancelled() => {
                info!("Received SIGINT, stopping");
                break;
            }

            notification = connection_manager_notification_receiver.recv() => {
                process_connection_manager_notification(notification, gossip, rpc_handler).await;
            }

            notification = gossip_notification_receiver.recv() => {
                process_gossip_notification(notification, connection_manager, gossip, scheduler).await;
            }

            notification = scheduler_notification_receiver.recv() => {
                process_scheduler_notification(notification, gossip).await;
            }

            notification = blockchain_monitor_notification_receiver.recv() => {
                process_blockchain_monitor_notification(notification, scheduler, request_signer_cache).await;
            }

            notification = gateway_notification_receiver.recv() => {
                handle_gateway_notification(notification, usage_aggregator);
            }

            notification = runtime_notification_receiver.recv() => {
                handle_runtime_notification(notification, usage_aggregator);
            }
        }
    }
}

async fn process_connection_manager_notification(
    notification: Option<ConnectionManagerNotification>,
    gossip: &dyn Gossip,
    rpc_handler: &dyn RpcHandler,
) {
    match notification {
        None => (), // TODO
        Some(ConnectionManagerNotification::NewConnectionAvailable(id)) => {
            debug!("New connection available: {}", id)
        }
        Some(ConnectionManagerNotification::ConnectionClosed(id)) => {
            debug!("Connection closed: {}", id)
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
            rpc_handler.request_received(id, req_id, bytes);
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
    request_signer_cache: &dyn RequestSignerCache,
) {
    match notification {
        None => (), // TODO
        Some(BlockchainMonitorNotification::StacksAvailable(stacks)) => {
            debug!("Stacks available: {stacks:?}");
            request_signer_cache
                .stacks_available(stacks.iter().map(|s| (s.id(), s.owner())).collect())
                .await
                .unwrap();
            scheduler.stacks_available(stacks.clone()).await.unwrap();
        }
        Some(BlockchainMonitorNotification::StacksRemoved(stack_ids)) => {
            debug!("Stacks removed: {stack_ids:?}");
            request_signer_cache
                .stacks_removed(stack_ids.clone())
                .await
                .unwrap();
            scheduler.stacks_removed(stack_ids).await.unwrap();
        }
        Some(BlockchainMonitorNotification::RequestSignersAvailable(signers)) => {
            debug!("Request signers available: {signers:?}");
            request_signer_cache
                .signers_available(signers)
                .await
                .unwrap();
        }
        Some(BlockchainMonitorNotification::RequestSignersRemoved(signers)) => {
            debug!("Request signers removed: {signers:?}");
            request_signer_cache.signers_removed(signers).await.unwrap();
        }
    }
}

fn handle_gateway_notification(
    notification: Option<mu_gateway::Notification>,
    usage_aggregator: &dyn UsageAggregator,
) {
    let mu_gateway::Notification::ReportUsage {
        stack_id,
        traffic,
        requests,
    } = notification.unwrap();

    usage_aggregator.register_usage(
        stack_id,
        vec![
            Usage::GatewayRequests { count: requests },
            Usage::GatewayTraffic {
                size_bytes: traffic,
            },
        ],
    );
}

fn handle_runtime_notification(
    notification: Option<mu_runtime::Notification>,
    usage_aggregator: &dyn UsageAggregator,
) {
    let mu_runtime::Notification::ReportUsage(stack_id, usage) = notification.unwrap();

    usage_aggregator.register_usage(
        stack_id,
        vec![
            Usage::DBRead {
                weak_reads: usage.db_weak_reads,
                strong_reads: usage.db_strong_reads,
            },
            Usage::DBWrite {
                weak_writes: usage.db_weak_writes,
                strong_writes: usage.db_strong_writes,
            },
            Usage::FunctionMBInstructions {
                memory_megabytes: usage.memory_megabytes,
                instructions: usage.function_instructions,
            },
        ],
    );
}
