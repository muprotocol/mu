use std::sync::Arc;

use anyhow::{bail, Context, Result};
use log::{debug, trace};
use mu_runtime::Runtime;
use mu_stack::{FunctionID, StackID};
use musdk_common::{Request, Response};
use rand::seq::SliceRandom;
use tokio::sync::RwLock;

use crate::{
    network::{
        connection_manager::ConnectionManager, gossip::Gossip, rpc_handler::RpcHandler,
        NodeConnection,
    },
    stack::scheduler::{Scheduler, StackDeploymentStatus},
};

#[derive(Clone, Debug)]
enum RoutingTarget {
    NotDeployed,
    Local,
    Remote(NodeConnection),
}

async fn get_route(
    stack_id: StackID,
    scheduler: &dyn Scheduler,
    gossip: &dyn Gossip,
) -> Result<RoutingTarget> {
    let deployment_status = scheduler
        .get_deployment_status(stack_id)
        .await
        .context("Failed to get deployment status")?;

    match deployment_status {
        StackDeploymentStatus::Unknown | StackDeploymentStatus::NotDeployed => {
            Ok(RoutingTarget::NotDeployed)
        }

        StackDeploymentStatus::DeployedToSelf { .. } => Ok(RoutingTarget::Local),

        StackDeploymentStatus::DeployedToOthers { deployed_to } => {
            let Some(invocation_target) = deployed_to.choose(&mut rand::thread_rng()) else {
                bail!("Internal error: no deployment targets");
            };

            let connection = gossip
                .get_connection(*invocation_target)
                .await
                .context("Failed to get connection to invocation target node")?;

            match connection {
                None => bail!("Scheduler reported stack is deployed to {invocation_target} but the hash is not known"),
                Some(c) => Ok(RoutingTarget::Remote(c)),
            }
        }
    }
}

pub async fn route_request(
    function_id: FunctionID,
    request: Request<'_>,
    connection_manager: Box<dyn ConnectionManager>,
    gossip: Box<dyn Gossip>,
    scheduler: Arc<RwLock<Option<Box<dyn Scheduler>>>>,
    rpc_handler: Box<dyn RpcHandler>,
    runtime: Box<dyn Runtime>,
) -> Result<Response<'static>> {
    trace!("Request received for {function_id}, will check deployment status");

    let scheduler_guard = scheduler.read().await;
    let scheduler = scheduler_guard.as_ref().unwrap().as_ref();
    let route = get_route(function_id.assembly_id.stack_id, scheduler, gossip.as_ref())
        .await
        .context("Failed to find route")?;
    drop(scheduler_guard);

    debug!(
        "Deployment status of stack {} is {:?}",
        function_id.assembly_id.stack_id, route
    );

    match route {
        RoutingTarget::NotDeployed => bail!("Stack not deployed"),
        RoutingTarget::Local => runtime
            .invoke_function(function_id, request)
            .await
            .map_err(Into::into),
        RoutingTarget::Remote(node_connection) => {
            let (connection_id, new_connection) = match node_connection {
                NodeConnection::Established(id) => (id, false),
                NodeConnection::NotEstablished(address) => {
                    // TODO! Does connecting to the target node here cause the gossip module to expect heartbeats?

                    // TODO should pool these connections so we don't do a connection handshake
                    // for each user request. QUIC is faster only if you're using an already open
                    // connection.
                    trace!("No connection to target node, will establish new connection");
                    let connection_id = connection_manager
                        .connect(address.address, address.port)
                        .await
                        .context("Failed to connect to invocation target node")?;

                    (connection_id, true)
                }
            };

            trace!("Sending request");
            let response = rpc_handler
                .send_execute_function(connection_id, function_id, request)
                .await
                .context("Error in remote function invocation");
            trace!("Response received");

            if new_connection {
                trace!("Will disconnect new connection");
                // Nothing to do if disconnecting errors out
                let _ = connection_manager.disconnect(connection_id).await;
            }

            response
        }
    }
}
