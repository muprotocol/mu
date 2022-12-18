use anyhow::{bail, Context, Result};
use mu_stack::StackID;
use rand::seq::SliceRandom;

use crate::{
    network::{gossip::Gossip, NodeConnection},
    stack::scheduler::{Scheduler, StackDeploymentStatus},
};

#[derive(Clone, Debug)]
pub enum RoutingTarget {
    NotDeployed,
    Local,
    Remote(NodeConnection),
}

pub async fn get_route(
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
