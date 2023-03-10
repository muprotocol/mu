pub mod api;
pub mod infrastructure;
pub mod network;
pub mod request_routing;
pub mod stack;

use std::{process, sync::Arc, time::SystemTime};

use anyhow::{Context, Result};
use async_trait::async_trait;
use log::*;
use mailbox_processor::NotificationChannel;
use mu_runtime::Runtime;
use network::{
    membership::Membership,
    rpc_handler::{self, RpcHandler, RpcRequestHandler},
};
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
        connection_manager::{self, ConnectionManagerNotification},
        membership, NodeAddress,
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
        membership_config,
        db_config,
        storage_config,
        gateway_manager_config,
        log_config,
        partial_runtime_config,
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

    let usage_aggregator = stack::usage_aggregator::start();

    let (blockchain_monitor, mut blockchain_monitor_notification_receiver, region_config) =
        blockchain_monitor::start(blockchain_monitor_config, usage_aggregator.clone())
            .await
            .context("Failed to start blockchain monitor")?;

    let database_manager = mu_db::start(db_config).await?;

    let storage_manager = mu_storage::start(&storage_config).await?;

    let runtime_config =
        partial_runtime_config.complete(region_config.max_giga_instructions_per_call);
    let (runtime, mut runtime_notification_receiver) = mu_runtime::start(
        database_manager.clone(),
        storage_manager.clone(),
        runtime_config,
    )
    .await
    .context("Failed to initiate runtime")?;

    let rpc_handler = rpc_handler::new(
        connection_manager.clone(),
        RpcRequestHandlerImpl {
            runtime: runtime.clone(),
        },
    );

    let (membership, mut membership_notification_receiver, known_nodes) = membership::start(
        my_node.clone(),
        membership_config,
        region_config.id,
        database_manager.clone(),
    )
    .await
    .context("Failed to start membership")?;

    let request_signer_cache = request_signer_cache::start();

    let scheduler_ref = Arc::new(RwLock::new(None));
    let (gateway_manager, mut gateway_notification_receiver) = mu_gateway::start(
        gateway_manager_config,
        api::service_factory(),
        Some(api::DependencyAccessor {
            request_signer_cache: request_signer_cache.clone(),
        }),
        {
            let connection_manager = connection_manager.clone();
            let membership = membership.clone();
            let scheduler_ref = scheduler_ref.clone();
            let rpc_handler = rpc_handler.clone();
            let runtime = runtime.clone();

            move |f, r| {
                Box::pin(request_routing::route_request(
                    f,
                    r,
                    connection_manager.clone(),
                    membership.clone(),
                    scheduler_ref.clone(),
                    rpc_handler.clone(),
                    runtime.clone(),
                ))
            }
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
        known_nodes
            .into_iter()
            .map(|a| (a.0.get_hash(), a.1))
            .collect(),
        vec![],
        scheduler_notification_channel,
        runtime.clone(),
        gateway_manager.clone(),
        database_manager.clone(),
        storage_manager.clone(),
    );

    info!("Will start to schedule stacks now");
    scheduler.ready_to_schedule_stacks().await?;

    *scheduler_ref.write().await = Some(scheduler.clone());

    glue_modules(
        cancellation_token,
        connection_manager_notification_receiver,
        membership.as_ref(),
        &mut membership_notification_receiver,
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
        .stop()
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

    trace!("Stopping membership");
    membership
        .stop()
        .await
        .context("Failed to stop membership")?;

    trace!("Stopping connection manager");
    connection_manager
        .stop()
        .await
        .context("Failed to stop connection manager")?;

    info!("Goodbye!");

    Ok(())
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
    mut connection_manager_notification_receiver: mpsc::UnboundedReceiver<
        ConnectionManagerNotification,
    >,
    membership: &dyn Membership,
    membership_notification_receiver: &mut mpsc::UnboundedReceiver<membership::Notification>,
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
                process_connection_manager_notification(notification, rpc_handler).await;
            }

            notification = membership_notification_receiver.recv() => {
                process_membership_notification(notification, scheduler).await;
            }

            notification = scheduler_notification_receiver.recv() => {
                process_scheduler_notification(notification, membership).await;
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

async fn process_membership_notification(
    notification: Option<membership::Notification>,
    scheduler: &dyn Scheduler,
) {
    match notification {
        None => (), // TODO
        Some(membership::Notification::NodeDiscovered(node)) => {
            debug!("Node discovered: {node}");
            scheduler.node_discovered(node.get_hash()).await.unwrap(); // TODO: unwrap
        }
        Some(membership::Notification::NodeDied(node, reason)) => {
            debug!("Node{node} died due to {reason:?}",);
            scheduler.node_died(node).await.unwrap(); // TODO: unwrap
        }
        Some(membership::Notification::NodeStacksChanged {
            node,
            added,
            removed,
        }) => {
            if !added.is_empty() {
                debug!("Node deployed stacks: {node} <- {added:?}");
                scheduler.node_deployed_stacks(node, added).await.unwrap(); // TODO: unwrap
            }

            if !removed.is_empty() {
                debug!("Node undeployed stack: {node} <- {removed:?}");
                scheduler
                    .node_undeployed_stacks(node, removed)
                    .await
                    .unwrap(); // TODO: unwrap
            }
        }
    }
}

async fn process_scheduler_notification(
    notification: Option<SchedulerNotification>,
    membership: &dyn Membership,
) {
    match notification {
        None => (), // TODO
        Some(SchedulerNotification::StackDeployed(id)) => {
            debug!("Deployed stack {id}");
            membership.stack_deployed_locally(id).await.unwrap(); // TODO: unwrap
        }
        Some(SchedulerNotification::StackUndeployed(id)) => {
            debug!("Undeployed stack {id}");
            membership.stack_undeployed_locally(id).await.unwrap(); // TODO: unwrap
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
        Some(BlockchainMonitorNotification::StacksRemoved(stacks)) => {
            debug!("Stacks removed: {stacks:?}");
            request_signer_cache
                .stacks_removed(stacks.iter().map(|s| s.0).collect())
                .await
                .unwrap();
            scheduler.stacks_removed(stacks).await.unwrap();
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
