use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::{debug, error, info, trace, warn};
use mailbox_processor::{callback::CallbackMailboxProcessor, NotificationChannel, ReplyChannel};
use mu_common::replace_with::ReplaceWithDefault;
use mu_gateway::GatewayManager;
use mu_runtime::Runtime;
use num::BigInt;
use serde::Deserialize;

use crate::{infrastructure::config::ConfigDuration, mudb::DbManager, network::NodeHash};

use mu_stack::{Stack, StackID};

pub enum StackDeploymentStatus {
    DeployedToSelf { deployed_to_others: Vec<NodeHash> },
    DeployedToOthers { deployed_to: Vec<NodeHash> },
    NotDeployed,
    Unknown,
}

#[async_trait]
#[clonable]
pub trait Scheduler: Clone + Send + Sync {
    async fn node_discovered(&self, node: NodeHash) -> Result<()>;
    async fn node_died(&self, node: NodeHash) -> Result<()>;
    async fn node_deployed_stacks(&self, node: NodeHash, stack_ids: Vec<StackID>) -> Result<()>;
    async fn node_undeployed_stacks(&self, node: NodeHash, stack_ids: Vec<StackID>) -> Result<()>;

    // TODO: implement stack updates
    async fn stacks_available(&self, stacks: Vec<(StackID, Stack)>) -> Result<()>;
    async fn stacks_removed(&self, ids: Vec<StackID>) -> Result<()>;

    /// We start scheduling stacks after a delay, to make sure we have
    /// an up-to-date view of the cluster.
    async fn ready_to_schedule_stacks(&self) -> Result<()>;

    async fn get_deployment_status(&self, stack_id: StackID) -> Result<StackDeploymentStatus>;

    // This function currently doesn't fail, but we keep the return type
    // a `Result<()>` so we can later implement custom stopping logic.
    async fn stop(&self) -> Result<()>;
}

pub enum SchedulerNotification {
    StackDeployed(StackID),
    StackUndeployed(StackID),
    FailedToDeployStack(StackID),
}

#[derive(Deserialize)]
pub struct SchedulerConfig {
    tick_interval: ConfigDuration,
}

enum SchedulerMessage {
    NodeDiscovered(NodeHash),
    NodeDied(NodeHash),
    NodeDeployedStacks(NodeHash, Vec<StackID>),
    NodeUndeployedStacks(NodeHash, Vec<StackID>),

    StacksAvailable(Vec<(StackID, Stack)>),
    StacksRemoved(Vec<StackID>),

    ReadyToScheduleStacks,

    GetDeploymentStatus(StackID, ReplyChannel<StackDeploymentStatus>),

    // We could just update the state every time a message arrives,
    // but we need to be able to cache operations for the duration
    // between when the scheduler starts and when it starts scheduling
    // stacks (via the `ReadyToScheduleStacks` message above.) To
    // prevent the entire code from having to handle the two separate
    // cases, we just use the `Tick` message to update deployments.
    Tick,
}

#[derive(Clone)]
struct SchedulerImpl {
    mailbox: CallbackMailboxProcessor<SchedulerMessage>,
}

#[async_trait]
impl Scheduler for SchedulerImpl {
    async fn node_discovered(&self, node: NodeHash) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::NodeDiscovered(node))
            .await
            .map_err(Into::into)
    }

    async fn node_died(&self, node: NodeHash) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::NodeDied(node))
            .await
            .map_err(Into::into)
    }

    async fn node_deployed_stacks(&self, node: NodeHash, stack_ids: Vec<StackID>) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::NodeDeployedStacks(node, stack_ids))
            .await
            .map_err(Into::into)
    }

    async fn node_undeployed_stacks(&self, node: NodeHash, stack_ids: Vec<StackID>) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::NodeUndeployedStacks(node, stack_ids))
            .await
            .map_err(Into::into)
    }

    async fn stacks_available(&self, stacks: Vec<(StackID, Stack)>) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::StacksAvailable(stacks))
            .await
            .map_err(Into::into)
    }

    async fn stacks_removed(&self, ids: Vec<StackID>) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::StacksRemoved(ids))
            .await
            .map_err(Into::into)
    }

    async fn ready_to_schedule_stacks(&self) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::ReadyToScheduleStacks)
            .await
            .map_err(Into::into)
    }

    async fn get_deployment_status(&self, stack_id: StackID) -> Result<StackDeploymentStatus> {
        self.mailbox
            .post_and_reply(|r| SchedulerMessage::GetDeploymentStatus(stack_id, r))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox.clone().stop().await;
        Ok(())
    }
}

enum StackDeployment {
    /// Representative of a stack ID we've seen in the heartbeats of other
    /// nodes, but the stack definition for which we haven't received yet.
    Unknown { deployed_to: HashSet<NodeHash> },

    /// An undeployed stack, which we may or may not want to deploy locally
    Undeployed { stack: Stack },

    /// A stack with a "deployment candidate" which is not the current node.
    /// A deployment candidate is a node with less hash distance to the
    /// stack in question. It will be the node with the least distance to
    /// the stack *at the time we discover the stack*, but *won't be kept up
    /// to date*. As long as the deployment candidate node is alive, we will
    /// never have to deploy the stack. If it does die, we will transition
    /// the stack to the undeployed state and either deploy it locally or
    /// find another deployment candidate.
    HasDeploymentCandidate {
        stack: Stack,
        deployment_candidate: NodeHash,
    },

    /// A stack that has been deployed locally. It's possible that a stack
    /// deployed locally has also been deployed to other nodes, and if this
    /// situation does arise, we'll need to resolve the conflict by dropping
    /// the stack if the other node is closer, or waiting for the other node
    /// to drop it if we're closer.
    DeployedToSelf {
        stack: Stack,
        deployed_to_others: HashSet<NodeHash>,
    },

    /// A stack that is deployed to other nodes, and which we have no
    /// interest in deploying locally.
    DeployedToOthers {
        stack: Stack,
        deployed_to: HashSet<NodeHash>,
    },
}

struct SchedulerState {
    my_hash: NodeHash,
    known_nodes: HashSet<NodeHash>,
    stacks: HashMap<StackID, StackDeployment>,
    reevaluate_on_next_tick: HashSet<StackID>,
    ready_to_schedule: bool,
    notification_channel: NotificationChannel<SchedulerNotification>,
    runtime: Box<dyn Runtime>,
    gateway_manager: Box<dyn GatewayManager>,
    database_manager: Box<dyn DbManager>,
}

#[allow(clippy::too_many_arguments)]
pub fn start(
    config: SchedulerConfig,
    my_hash: NodeHash,
    known_nodes: Vec<NodeHash>,
    available_stacks: Vec<(StackID, Stack)>,
    notification_channel: NotificationChannel<SchedulerNotification>,
    runtime: Box<dyn Runtime>,
    gateway_manager: Box<dyn GatewayManager>,
    database_manager: Box<dyn DbManager>,
) -> Box<dyn Scheduler> {
    let tick_interval = *config.tick_interval;

    let mailbox = CallbackMailboxProcessor::start(
        step,
        SchedulerState {
            my_hash,
            stacks: available_stacks
                .into_iter()
                .map(|(id, stack)| (id, StackDeployment::Undeployed { stack }))
                .collect(),
            reevaluate_on_next_tick: HashSet::new(),
            ready_to_schedule: false,
            known_nodes: known_nodes.into_iter().collect(),
            notification_channel,
            runtime,
            gateway_manager,
            database_manager,
        },
        10000,
    );

    let res = SchedulerImpl { mailbox };

    let res_clone = res.clone();
    tokio::spawn(async move { generate_tick(res_clone, tick_interval).await });

    Box::new(res)
}

async fn generate_tick(scheduler: SchedulerImpl, interval: Duration) {
    let mut timer = tokio::time::interval(interval);
    // Timers tick once immediately
    timer.tick().await;

    loop {
        timer.tick().await;
        if let Err(mailbox_processor::Error::MailboxStopped) =
            scheduler.mailbox.post(SchedulerMessage::Tick).await
        {
            return;
        }
    }
}

async fn step(
    _mb: CallbackMailboxProcessor<SchedulerMessage>,
    msg: SchedulerMessage,
    mut state: SchedulerState,
) -> SchedulerState {
    match msg {
        SchedulerMessage::ReadyToScheduleStacks => state.ready_to_schedule = true,

        SchedulerMessage::NodeDiscovered(hash) => {
            state.known_nodes.insert(hash);
        }

        SchedulerMessage::NodeDied(node) => {
            state.known_nodes.remove(&node);

            // TODO: implement indexing to prevent looping over all deployments
            for (id, deployment) in state.stacks.iter_mut() {
                match deployment {
                    StackDeployment::Unknown { deployed_to, .. } => {
                        if deployed_to.remove(&node) {
                            state.reevaluate_on_next_tick.insert(*id);
                        }
                    }

                    StackDeployment::DeployedToSelf {
                        deployed_to_others, ..
                    } => {
                        if deployed_to_others.remove(&node) {
                            state.reevaluate_on_next_tick.insert(*id);
                        }
                    }

                    StackDeployment::Undeployed { .. } => (),

                    StackDeployment::HasDeploymentCandidate {
                        stack,
                        deployment_candidate,
                    } => {
                        // If the dead node was a deployment candidate, clear it so we
                        // can scan for a new candidate on the next tick.
                        if *deployment_candidate == node {
                            *deployment = StackDeployment::Undeployed {
                                stack: stack.take_and_replace_default(),
                            };
                            state.reevaluate_on_next_tick.insert(*id);
                        }
                    }

                    StackDeployment::DeployedToOthers { stack, deployed_to } => {
                        if deployed_to.remove(&node) && deployed_to.is_empty() {
                            // No longer deployed to any nodes, so transition to undeployed
                            *deployment = StackDeployment::Undeployed {
                                stack: stack.take_and_replace_default(),
                            };
                            state.reevaluate_on_next_tick.insert(*id);
                        }
                    }
                }
            }
        }

        SchedulerMessage::NodeDeployedStacks(node, stack_ids) => {
            for stack_id in stack_ids {
                state.reevaluate_on_next_tick.insert(stack_id);
                match state.stacks.entry(stack_id) {
                    Entry::Vacant(vac) => {
                        let mut deployed_to = HashSet::new();
                        deployed_to.insert(node);
                        vac.insert(StackDeployment::Unknown { deployed_to });
                    }

                    Entry::Occupied(mut occ) => match occ.get_mut() {
                        StackDeployment::DeployedToOthers { deployed_to, .. }
                        | StackDeployment::DeployedToSelf {
                            deployed_to_others: deployed_to,
                            ..
                        }
                        | StackDeployment::Unknown { deployed_to, .. } => {
                            deployed_to.insert(node);
                        }

                        StackDeployment::HasDeploymentCandidate { stack, .. }
                        | StackDeployment::Undeployed { stack } => {
                            let mut deployed_to = HashSet::new();
                            deployed_to.insert(node);
                            let stack = stack.take_and_replace_default();
                            occ.insert(StackDeployment::DeployedToOthers { stack, deployed_to });
                        }
                    },
                }
            }
        }

        SchedulerMessage::NodeUndeployedStacks(node, stack_ids) => {
            for stack_id in stack_ids {
                state.reevaluate_on_next_tick.insert(stack_id);
                match state.stacks.entry(stack_id) {
                    Entry::Vacant(_) => {
                        // We should have received a notification of the node deploying the stack
                        // before, so this is an error case
                        warn!("Received undeployment notification for stack {stack_id} on node {node}, but we don't know this stack");
                    }

                    Entry::Occupied(mut occ) => match occ.get_mut() {
                        StackDeployment::DeployedToSelf {
                            deployed_to_others: deployed_to,
                            ..
                        } => {
                            if !deployed_to.remove(&node) {
                                warn!("Received undeployment notification for stack {stack_id} on node {node}, but we didn't know it was scheduled there");
                            }
                        }

                        StackDeployment::Unknown { deployed_to, .. } => {
                            if !deployed_to.remove(&node) {
                                warn!("Received undeployment notification for stack {stack_id} on node {node}, but we didn't know it was scheduled there");
                            }

                            if deployed_to.is_empty() {
                                occ.remove();
                            }
                        }

                        StackDeployment::HasDeploymentCandidate { .. }
                        | StackDeployment::Undeployed { .. } => {
                            warn!("Received undeployment notification for stack {stack_id} on node {node}, but we didn't know it was scheduled at all");
                        }

                        StackDeployment::DeployedToOthers { stack, deployed_to } => {
                            if deployed_to.remove(&node) && deployed_to.is_empty() {
                                let stack = stack.take_and_replace_default();
                                occ.insert(StackDeployment::Undeployed { stack });
                            }
                        }
                    },
                }
            }
        }

        SchedulerMessage::StacksAvailable(stacks) => {
            for (id, stack) in stacks {
                state.reevaluate_on_next_tick.insert(id);

                // As soon as we get a stack definition, we want to deploy its gateways so we can
                // route new requests to that stack to the correct node.
                info!("Deploying gateways for {id}");
                deploy_gateways(id, &stack, state.gateway_manager.as_ref()).await;

                match state.stacks.entry(id) {
                    Entry::Vacant(vac) => {
                        vac.insert(StackDeployment::Undeployed { stack });
                    }

                    Entry::Occupied(mut occ) => match occ.get_mut() {
                        StackDeployment::Unknown { deployed_to } => {
                            if deployed_to.is_empty() {
                                warn!("Stack {id} was in Unknown state with empty deployed_to");
                                occ.insert(StackDeployment::Undeployed { stack });
                            } else {
                                let deployed_to = deployed_to.take_and_replace_default();
                                occ.insert(StackDeployment::DeployedToOthers {
                                    stack,
                                    deployed_to,
                                });
                            }
                        }

                        _ => warn!("Received known stack with ID {id}"),
                    },
                }
            }
        }

        SchedulerMessage::StacksRemoved(ids) => {
            for id in ids {
                undeploy_gateways(id, state.gateway_manager.as_ref()).await;

                match state.stacks.entry(id) {
                    Entry::Vacant(_) => warn!("Unknown stack {id} was removed"),

                    Entry::Occupied(mut occ) => match occ.get_mut() {
                        StackDeployment::Unknown { .. } => {
                            warn!("Unknown stack {id} was removed");
                            occ.remove();
                        }

                        StackDeployment::DeployedToSelf { .. } => {
                            undeploy_stack(id, &state.notification_channel).await;
                            occ.remove();
                        }

                        StackDeployment::DeployedToOthers { .. }
                        | StackDeployment::HasDeploymentCandidate { .. }
                        | StackDeployment::Undeployed { .. } => {
                            occ.remove();
                        }
                    },
                }
            }
        }

        SchedulerMessage::GetDeploymentStatus(stack_id, r) => r.reply(
            state
                .stacks
                .get(&stack_id)
                .map(|s| match s {
                    StackDeployment::Undeployed { .. }
                    | StackDeployment::HasDeploymentCandidate { .. } => {
                        StackDeploymentStatus::NotDeployed
                    }

                    StackDeployment::Unknown { deployed_to }
                    | StackDeployment::DeployedToOthers { deployed_to, .. } => {
                        StackDeploymentStatus::DeployedToOthers {
                            deployed_to: deployed_to.iter().cloned().collect(),
                        }
                    }

                    StackDeployment::DeployedToSelf {
                        deployed_to_others, ..
                    } => StackDeploymentStatus::DeployedToSelf {
                        deployed_to_others: deployed_to_others.iter().cloned().collect(),
                    },
                })
                .unwrap_or(StackDeploymentStatus::Unknown),
        ),

        SchedulerMessage::Tick => {
            tick(&mut state).await;
        }
    }

    state
}

async fn tick(state: &mut SchedulerState) {
    if !state.ready_to_schedule {
        trace!("Not ready to schedule stacks, won't tick");
        return;
    }

    if !state.reevaluate_on_next_tick.is_empty() {
        debug!("Scheduler tick");
    }

    for id in &state.reevaluate_on_next_tick {
        if let Entry::Occupied(mut occ) = state.stacks.entry(*id) {
            debug!("Updating stack {id}");
            match occ.get_mut() {
                StackDeployment::Undeployed { stack } => {
                    debug!("Is undeployed, will evaluate closest node");
                    match get_closest_node(*id, state.my_hash, state.known_nodes.iter()) {
                        GetClosestNodeResult::Me => {
                            info!("Deploying stack {id} locally");
                            match deploy_stack(
                                *id,
                                stack.clone(),
                                &state.notification_channel,
                                state.runtime.as_ref(),
                                state.database_manager.as_ref(),
                            )
                            .await
                            {
                                Err(f) => {
                                    error!("Failed to deploy stack {id} due to: {f}");
                                }

                                Ok(()) => {
                                    let stack = stack.take_and_replace_default();
                                    occ.insert(StackDeployment::DeployedToSelf {
                                        stack,
                                        deployed_to_others: Default::default(),
                                    });
                                }
                            }
                        }

                        GetClosestNodeResult::Other(node) => {
                            debug!(
                                "Closest node is remote {node}, will set as deployment candidate"
                            );
                            let stack = stack.take_and_replace_default();
                            occ.insert(StackDeployment::HasDeploymentCandidate {
                                stack,
                                deployment_candidate: node,
                            });
                        }
                    }
                }

                StackDeployment::DeployedToSelf {
                    stack,
                    deployed_to_others,
                } => {
                    debug!("Is deployed to self");
                    if !deployed_to_others.is_empty() {
                        debug!("Is also deployed to remotes, will evaluate closest node");
                        if let GetClosestNodeResult::Other(node) =
                            get_closest_node(*id, state.my_hash, deployed_to_others.iter())
                        {
                            info!("Stack {id} was deployed to closer node {node}, will undeploy");
                            undeploy_stack(*id, &state.notification_channel).await;

                            let stack = stack.take_and_replace_default();
                            let deployed_to = deployed_to_others.take_and_replace_default();
                            occ.insert(StackDeployment::DeployedToOthers { stack, deployed_to });
                        } else {
                            debug!("I'm closest, nothing to do");
                        }
                    }
                }

                StackDeployment::DeployedToOthers { stack, deployed_to } => {
                    debug!("Is deployed to others, will evaluate closest node");
                    match get_closest_node(*id, state.my_hash, deployed_to.iter()) {
                        GetClosestNodeResult::Me => {
                            info!("I am closest to stack {id}, will deploy locally");
                            match deploy_stack(
                                *id,
                                stack.clone(),
                                &state.notification_channel,
                                state.runtime.as_ref(),
                                state.database_manager.as_ref(),
                            )
                            .await
                            {
                                Err(f) => {
                                    error!("Failed to deploy stack {id} due to: {f}");
                                }

                                Ok(()) => {
                                    let stack = stack.take_and_replace_default();
                                    let deployed_to_others = deployed_to.take_and_replace_default();
                                    occ.insert(StackDeployment::DeployedToSelf {
                                        stack,
                                        deployed_to_others,
                                    });
                                }
                            }
                        }

                        GetClosestNodeResult::Other(node) => {
                            debug!("Is closest to node {node}, nothing to do");
                        }
                    }
                }

                // Nothing to do if a stack has a live deployment candidate
                StackDeployment::HasDeploymentCandidate { .. } => {
                    debug!("Has deployment candidate, nothing to do")
                }

                // Nothing to do with an unknown stack; even if we are closer, we
                // must wait for the stack's definition to become available
                StackDeployment::Unknown { .. } => {
                    debug!("Stack definition not available, nothing to do")
                }
            }
        } else {
            debug!("Stack {id} was in reevaluation list but had no entry");
        }
    }

    state.reevaluate_on_next_tick.clear();
}

async fn deploy_gateways(id: StackID, stack: &Stack, gateway_manager: &dyn GatewayManager) {
    if let Err(f) = super::deploy::deploy_gateways(id, stack, gateway_manager).await {
        warn!("Failed to deploy gateways of stack {id} due to: {f:?}");
    }
}

async fn undeploy_gateways(_id: StackID, _gateway_manager: &dyn GatewayManager) {
    error!("Undeployment not implemented yet");
}

async fn deploy_stack(
    id: StackID,
    stack: Stack,
    notification_channel: &NotificationChannel<SchedulerNotification>,
    runtime: &dyn Runtime,
    database_manager: &dyn DbManager,
) -> Result<()> {
    let db_client = database_manager.make_client().await?;
    match super::deploy::deploy(id, stack, runtime, db_client.as_ref()).await {
        Err(f) => {
            notification_channel.send(SchedulerNotification::FailedToDeployStack(id));
            Err(f.into())
        }

        Ok(()) => {
            notification_channel.send(SchedulerNotification::StackDeployed(id));
            Ok(())
        }
    }
}

async fn undeploy_stack(
    _id: StackID,
    _notification_channel: &NotificationChannel<SchedulerNotification>,
) {
    error!("Undeployment not implemented yet");
}

#[derive(Debug)]
enum GetClosestNodeResult {
    Me,
    Other(NodeHash),
}

fn get_closest_node<'a>(
    id: StackID,
    my_hash: NodeHash,
    others: impl Iterator<Item = &'a NodeHash>,
) -> GetClosestNodeResult {
    fn to_bigint(x: &[u8; 32]) -> BigInt {
        BigInt::from_bytes_le(num::bigint::Sign::Plus, x)
    }

    trace!("Determining closest node to {id}");

    let id_int = to_bigint(id.get_bytes());

    let mut min_distance = id_int.clone() ^ to_bigint(&my_hash.0);
    trace!("Distance to self: {min_distance:?}");
    let mut result = GetClosestNodeResult::Me;

    for hash in others {
        let distance = id_int.clone() ^ to_bigint(&hash.0);
        trace!("Distance to {hash}: {distance}");
        if distance < min_distance {
            min_distance = distance;
            result = GetClosestNodeResult::Other(*hash);
        }
    }

    trace!("Result: {result:?}");
    result
}
