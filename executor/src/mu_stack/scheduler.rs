use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use dyn_clonable::clonable;
use mailbox_processor::callback::CallbackMailboxProcessor;

use crate::network::gossip::NodeHash;

use super::{Stack, StackID};

#[async_trait]
#[clonable]
pub trait Scheduler: Clone {
    async fn node_discovered(&self, node: NodeHash) -> Result<()>;
    async fn node_died(&self, node: NodeHash) -> Result<()>;
    async fn node_deployed_stack(&self, node: NodeHash, stack_id: StackID) -> Result<()>;
    async fn node_undeployed_stack(&self, node: NodeHash, stack_id: StackID) -> Result<()>;

    // TODO: implement stack updates
    async fn stack_available(&self, id: StackID, stack: Stack) -> Result<()>;

    /// We start scheduling stacks after a delay, to make sure we have
    /// an up-to-date view of the cluster.
    async fn ready_to_schedule_stacks(&self) -> Result<()>;
    // This function currently doesn't fail, but we keep the return type
    // a `Result<()>` so we can later implement custom stopping logic.
    async fn stop(&self) -> Result<()>;
}

pub enum SchedulerNotifications {
    StackDeployed(StackID),
    StackUndeployed(StackID),
}

pub struct SchedulerConfig {
    tick_interval: Duration,
}

enum SchedulerMessage {
    NodeDiscovered(NodeHash),
    NodeDied(NodeHash),
    NodeDeployedStack(NodeHash, StackID),
    NodeUndeployedStack(NodeHash, StackID),

    StackAvailable(StackID, Stack),

    ReadyToScheduleStacks,
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

    async fn node_deployed_stack(&self, node: NodeHash, stack_id: StackID) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::NodeDeployedStack(node, stack_id))
            .await
            .map_err(Into::into)
    }

    async fn node_undeployed_stack(&self, node: NodeHash, stack_id: StackID) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::NodeUndeployedStack(node, stack_id))
            .await
            .map_err(Into::into)
    }

    async fn stack_available(&self, id: StackID, stack: Stack) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::StackAvailable(id, stack))
            .await
            .map_err(Into::into)
    }

    async fn ready_to_schedule_stacks(&self) -> Result<()> {
        self.mailbox
            .post(SchedulerMessage::ReadyToScheduleStacks)
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox.clone().stop().await;
        Ok(())
    }
}

struct StackDeployment<T> {
    id: StackID,
    stack: Stack,
    state: T,
}

struct Undeployed {
    deployment_candidate: Option<NodeHash>,
}

struct Deployed {
    currently_deployed_to: HashSet<NodeHash>,
}

struct SchedulerState {
    config: SchedulerConfig,
    own_hash: NodeHash,
    known_nodes: HashSet<NodeHash>,
    undeployed: Vec<StackDeployment<Undeployed>>,
    deployed_to_self: Vec<StackDeployment<Deployed>>,
    deployed_to_others: Vec<StackDeployment<Deployed>>,
    unknown_deployed_stacks: HashMap<StackID, Vec<NodeHash>>,
    ready_to_schedule: bool,
}

pub fn start(
    config: SchedulerConfig,
    own_hash: NodeHash,
    available_stacks: Vec<(StackID, Stack)>,
) -> impl Scheduler {
    let tick_interval = config.tick_interval;

    let mailbox = CallbackMailboxProcessor::start(
        step,
        SchedulerState {
            config,
            own_hash,
            undeployed: available_stacks
                .into_iter()
                .map(|(id, stack)| StackDeployment {
                    id,
                    stack,
                    state: Undeployed {
                        deployment_candidate: None,
                    },
                })
                .collect(),
            deployed_to_self: vec![],
            deployed_to_others: vec![],
            unknown_deployed_stacks: HashMap::new(),
            ready_to_schedule: false,
            known_nodes: HashSet::new(),
        },
        10000,
    );

    let res = SchedulerImpl { mailbox };

    let res_clone = res.clone();
    tokio::spawn(async move { generate_tick(res_clone, tick_interval).await });

    res
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

        SchedulerMessage::NodeDied(hash) => {
            state.known_nodes.remove(&hash);

            // clear the deployment candidate on stacks which were going to be deployed the dead node
            for undeployed in &mut state.undeployed {
                if undeployed.state.deployment_candidate == Some(hash) {
                    undeployed.state.deployment_candidate = None;
                }
            }

            // mark stacks that are deployed to the dead node as undeployed
            let mut i = 0;
            while i < state.deployed_to_others.len() {
                let dep = state.deployed_to_others.get_mut(i).unwrap();
                if dep.state.currently_deployed_to.remove(&hash)
                    && dep.state.currently_deployed_to.is_empty()
                {
                    let dep = state.deployed_to_others.remove(i);
                    state.undeployed.push(StackDeployment {
                        id: dep.id,
                        stack: dep.stack,
                        state: Undeployed {
                            deployment_candidate: None,
                        },
                    });
                } else {
                    i += 1;
                }
            }
        }

        SchedulerMessage::NodeDeployedStack(node, stack_id) => {
            todo!()
        }

        SchedulerMessage::NodeUndeployedStack(node, stack_id) => {
            todo!()
        }

        SchedulerMessage::StackAvailable(id, stack) => {
            todo!()
        }

        SchedulerMessage::Tick => {
            todo!()
        }
    }

    state
}
