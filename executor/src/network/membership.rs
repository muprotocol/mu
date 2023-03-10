// TODO: This is a quick-and-dirty replacement for the gossip module.
// There are many opportunities for improvement.

mod node_collection;
mod protos;

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use base58::ToBase58;
use dyn_clonable::clonable;
use log::{debug, error, info, warn};
use mailbox_processor::{callback::CallbackMailboxProcessor, NotificationChannel, ReplyChannel};
use mu_common::serde_support::ConfigDuration;
use mu_db::{DbClient, DbManager};
use mu_stack::StackID;
use protobuf::Message;
use serde::Deserialize;
use tokio::sync::mpsc;

use self::node_collection::NodeCollection;

use super::{NodeAddress, NodeHash};

const PKG_VERSION_MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const PKG_VERSION_MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
const PKG_VERSION_PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");

// Note: user keys are prefixed with the stack ID, which has a length
// prefix of its own, and can never be empty. By using a \0 in front,
// we make sure membership keys can never collide with user keys.
const DB_KEY_PREFIX: &[u8] = b"\0M";
const DB_KEY_UPPER_BOUND: &[u8] = b"\0N";

#[async_trait]
#[clonable]
pub trait Membership: Clone + Sync + Send {
    async fn get_nodes_and_stacks(&self) -> Result<Vec<(NodeAddress, Vec<StackID>)>>;
    async fn get_node(&self, hash: NodeHash) -> Result<Option<NodeAddress>>;
    async fn stop(&self) -> Result<()>;

    async fn stack_deployed_locally(&self, stack_id: StackID) -> Result<()>;
    async fn stack_undeployed_locally(&self, stack_id: StackID) -> Result<()>;
}

#[derive(Clone, Deserialize, Debug)]
pub struct MembershipConfig {
    pub update_interval: ConfigDuration,
    pub assume_dead_after: ConfigDuration,
}

enum MailboxMessage {
    StackDeployedLocally(StackID),
    StackUndeployedLocally(StackID),
    GetNodes(ReplyChannel<Vec<(NodeAddress, Vec<StackID>)>>),
    GetNode(NodeHash, ReplyChannel<Option<NodeAddress>>),
    Update,
    Stop,
}

pub enum Notification {
    NodeDiscovered(NodeAddress),
    NodeDied(NodeHash, NodeDeadReason),
    NodeStacksChanged {
        node: NodeHash,
        added: Vec<StackID>,
        removed: Vec<StackID>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum NodeDeadReason {
    DeadState,
    ReplacedByNewGeneration,
    MissedUpdate,
    MissingFromDb,
}

struct State {
    notification_channel: NotificationChannel<Notification>,
    db: Box<dyn DbClient>,

    nodes: NodeCollection,
    assume_dead_after: chrono::Duration,

    my_version: u32,
    my_address: NodeAddress,
    deployed_stacks: HashSet<StackID>,
    region_id: Vec<u8>,
}

#[derive(Debug)]
enum NodeState {
    Dead,
    Alive,
}

// The status information nodes write to the database
#[derive(Debug)]
struct NodeStatus {
    version: u32,
    address: NodeAddress,
    region_id: Vec<u8>,
    last_update: chrono::NaiveDateTime,
    state: NodeState,
    deployed_stacks: HashSet<StackID>,
}

impl NodeStatus {
    fn write_to_bytes(self) -> Result<Vec<u8>> {
        let s: protos::membership::NodeStatus = self.into();
        s.write_to_bytes()
            .context("Failed to serialize node status")
    }
}

// Each node's view of other nodes
#[derive(Debug)]
struct RemoteNodeInfo {
    #[allow(dead_code)]
    version: u32,
    address: NodeAddress,
    dead_reason: Option<NodeDeadReason>,
    deployed_stacks: HashSet<StackID>,
}

#[derive(Clone)]
struct MembershipImpl {
    mailbox: CallbackMailboxProcessor<MailboxMessage>,
}

#[async_trait]
impl Membership for MembershipImpl {
    async fn get_nodes_and_stacks(&self) -> Result<Vec<(NodeAddress, Vec<StackID>)>> {
        self.mailbox
            .post_and_reply(MailboxMessage::GetNodes)
            .await
            .map_err(Into::into)
    }

    async fn get_node(&self, node_hash: NodeHash) -> Result<Option<NodeAddress>> {
        self.mailbox
            .post_and_reply(|r| MailboxMessage::GetNode(node_hash, r))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox.post(MailboxMessage::Stop).await?;
        self.mailbox.clone().stop().await;
        Ok(())
    }

    async fn stack_deployed_locally(&self, stack_id: StackID) -> Result<()> {
        self.mailbox
            .post(MailboxMessage::StackDeployedLocally(stack_id))
            .await
            .map_err(Into::into)
    }

    async fn stack_undeployed_locally(&self, stack_id: StackID) -> Result<()> {
        self.mailbox
            .post(MailboxMessage::StackUndeployedLocally(stack_id))
            .await
            .map_err(Into::into)
    }
}

pub async fn start(
    my_address: NodeAddress,
    config: MembershipConfig,
    region_id: Vec<u8>,
    db_manager: Box<dyn DbManager>,
) -> Result<(
    Box<dyn Membership>,
    mpsc::UnboundedReceiver<Notification>,
    Vec<(NodeAddress, Vec<StackID>)>,
)> {
    info!("Starting membership");

    let (tx, rx) = NotificationChannel::new();
    let db_client = db_manager.make_client().await?;
    let my_version = PKG_VERSION_MAJOR.parse::<u32>().unwrap() * 1_000_000
        + PKG_VERSION_MINOR.parse::<u32>().unwrap() * 1_000
        + PKG_VERSION_PATCH.parse::<u32>().unwrap();
    let update_interval = *config.update_interval;
    let assume_dead_after = chrono::Duration::from_std(*config.assume_dead_after).unwrap();

    let now = chrono::Utc::now().naive_utc();

    let all_nodes = read_status_all(db_client.as_ref())
        .await
        .context("Failed to initialize membership")?
        .into_iter()
        .filter_map(|(_, v)| {
            if v.region_id != region_id {
                warn!(
                    "Found node {}:{} belonging to region {} in membership state, will ignore",
                    v.address.address,
                    v.address.port,
                    v.region_id.to_base58()
                );
                None
            } else if v.address.address == my_address.address && v.address.port == my_address.port {
                None
            } else {
                let dead_reason = get_dead_reason(&assume_dead_after, &now, &v);
                Some(RemoteNodeInfo {
                    version: v.version,
                    address: v.address,
                    dead_reason,
                    deployed_stacks: v.deployed_stacks,
                })
            }
        })
        .collect::<Vec<_>>();

    debug!("Found existing nodes: {all_nodes:?}");

    let live_nodes = all_nodes
        .iter()
        .filter_map(|n| {
            if n.dead_reason.is_some() {
                None
            } else {
                Some((
                    n.address.clone(),
                    n.deployed_stacks.iter().cloned().collect(),
                ))
            }
        })
        .collect();

    let state = State {
        notification_channel: tx,
        db: db_client,
        nodes: NodeCollection::new(all_nodes),
        assume_dead_after,
        my_version,
        my_address,
        deployed_stacks: Default::default(),
        region_id,
    };
    let mailbox = CallbackMailboxProcessor::start(body, state, 10000);

    let membership = MembershipImpl { mailbox };

    {
        let membership = membership.clone();
        tokio::spawn(async move { generate_tick(membership, update_interval).await });
    }

    Ok((Box::new(membership), rx, live_nodes))
}

async fn generate_tick(membership: MembershipImpl, interval: Duration) {
    let mut timer = tokio::time::interval(interval);

    // We don't skip the initial tick on purpose.
    loop {
        timer.tick().await;
        if let Err(mailbox_processor::Error::MailboxStopped) =
            membership.mailbox.post(MailboxMessage::Update).await
        {
            return;
        }
    }
}

async fn body(
    _mb: CallbackMailboxProcessor<MailboxMessage>,
    msg: MailboxMessage,
    mut state: State,
) -> State {
    fn get_if_alive(n: &RemoteNodeInfo) -> Option<&RemoteNodeInfo> {
        if n.dead_reason.is_none() {
            Some(n)
        } else {
            None
        }
    }
    match msg {
        MailboxMessage::GetNodes(r) => r.reply(
            state
                .nodes
                .get_nodes()
                .filter_map(|n| {
                    get_if_alive(n).map(|n| {
                        (
                            n.address.clone(),
                            n.deployed_stacks.iter().cloned().collect(),
                        )
                    })
                })
                .collect(),
        ),

        MailboxMessage::GetNode(hash, r) => r.reply(
            state
                .nodes
                .get_node(&hash)
                .and_then(|n| get_if_alive(n).map(|n| n.address.clone())),
        ),

        MailboxMessage::StackDeployedLocally(stack_id) => {
            state.deployed_stacks.insert(stack_id);
            debug!(
                "Stack {stack_id} was deployed locally, deployed stacks are: {:?}",
                state.deployed_stacks
            );
        }

        MailboxMessage::StackUndeployedLocally(stack_id) => {
            state.deployed_stacks.remove(&stack_id);
            debug!(
                "Stack {stack_id} was undeployed locally, deployed stacks are: {:?}",
                state.deployed_stacks
            );
        }

        MailboxMessage::Update => {
            if let Err(e) = perform_update(&mut state).await {
                error!("Failed to perform update: {e:?}");
            }
        }

        MailboxMessage::Stop => {
            if let Err(e) = mark_me_dead(&state).await {
                error!("Failed to update state to dead: {e:?}");
            }
        }
    }

    state
}

async fn perform_update(state: &mut State) -> Result<()> {
    let now = chrono::Utc::now().naive_utc();

    let my_status = NodeStatus {
        version: state.my_version,
        address: state.my_address.clone(),
        region_id: state.region_id.clone(),
        last_update: now,
        state: NodeState::Alive,
        deployed_stacks: state.deployed_stacks.iter().cloned().collect(),
    };
    write_status(state.db.as_ref(), my_status)
        .await
        .context("Failed to write my status to DB")?;

    let mut all_nodes = read_status_all(state.db.as_ref())
        .await
        .context("Failed to load node statuses from DB")?;

    all_nodes.retain(|_, v| v.region_id == state.region_id && v.address != state.my_address);

    let mut missing = vec![];
    for known in state.nodes.get_nodes() {
        if !all_nodes.contains_key(&(known.address.address, known.address.port)) {
            let hash = known.address.get_hash();
            state
                .notification_channel
                .send(Notification::NodeDied(hash, NodeDeadReason::MissingFromDb));
            missing.push(hash);
        }
    }
    for hash in missing {
        state.nodes.remove(&hash);
    }

    for new in all_nodes {
        let dead_reason = get_dead_reason(&state.assume_dead_after, &now, &new.1);

        match (state.nodes.get_by_address(&new.0), dead_reason) {
            (None, None) => {
                on_node_discovered(state, new.1);
            }

            (None, Some(_)) => debug!(
                "Discovered dead node {}:{}, will ignore",
                new.1.address.address, new.1.address.port
            ),

            (Some(existing), dead_reason)
                if existing.address.generation == new.1.address.generation =>
            {
                let hash = new.1.address.get_hash();

                // We want the discovery notification to happen before the stack updates,
                // but the dead notification to happen after.
                if dead_reason.is_none() && existing.dead_reason.is_some() {
                    debug!(
                        "Dead node {}:{} came back online",
                        new.1.address.address, new.1.address.port
                    );
                    state
                        .notification_channel
                        .send(Notification::NodeDiscovered(new.1.address.clone()));
                }

                let CompareDeployedStacksResult { added, removed } =
                    compare_deployed_stack_list(&existing.deployed_stacks, &new.1.deployed_stacks);
                if !added.is_empty() || !removed.is_empty() {
                    debug!(
                        "Node {}:{} deployed stacks updated, added: {added:?}, removed: {removed:?}",
                        new.1.address.address, new.1.address.port
                    );
                    state
                        .notification_channel
                        .send(Notification::NodeStacksChanged {
                            node: hash,
                            added: added.clone(),
                            removed: removed.clone(),
                        })
                }

                if let Some(dead_reason) = dead_reason {
                    if existing.dead_reason.is_none() {
                        debug!(
                            "Node {}:{} is dead due to {dead_reason:?}",
                            new.1.address.address, new.1.address.port
                        );
                        state.notification_channel.send(Notification::NodeDied(
                            existing.address.get_hash(),
                            dead_reason,
                        ));
                    }
                }

                state.nodes.update_in_place(&hash, |node| {
                    node.dead_reason = dead_reason;
                    node.deployed_stacks = new.1.deployed_stacks;
                });
            }

            (Some(existing), dead_reason)
                if existing.address.generation < new.1.address.generation =>
            {
                let existing_hash = existing.address.get_hash();
                if existing.dead_reason.is_none() {
                    debug!(
                        "Discovered newer generation of node {}:{}, marking old generation dead",
                        existing.address.address, existing.address.port
                    );
                    state.notification_channel.send(Notification::NodeDied(
                        existing_hash,
                        NodeDeadReason::ReplacedByNewGeneration,
                    ));
                }

                state.nodes.remove(&existing_hash);

                if dead_reason.is_none() {
                    on_node_discovered(state, new.1);
                }
            }

            (Some(existing), _) => debug!(
                "Discovered older generation of node{}:{}, ignoring",
                existing.address.address, existing.address.port
            ),
        }
    }

    Ok(())
}

fn on_node_discovered(state: &mut State, node: NodeStatus) {
    debug!("Node discovered: {node:?}");
    state
        .notification_channel
        .send(Notification::NodeDiscovered(node.address.clone()));
    assert!(state.nodes.insert(RemoteNodeInfo {
        version: node.version,
        address: node.address,
        dead_reason: None,
        deployed_stacks: node.deployed_stacks
    }));
}

fn get_dead_reason(
    assume_dead_after: &chrono::Duration,
    now: &chrono::NaiveDateTime,
    node: &NodeStatus,
) -> Option<NodeDeadReason> {
    match node.state {
        NodeState::Dead => Some(NodeDeadReason::DeadState),
        NodeState::Alive => {
            if now.signed_duration_since(node.last_update) < *assume_dead_after {
                None
            } else {
                Some(NodeDeadReason::MissedUpdate)
            }
        }
    }
}

async fn mark_me_dead(state: &State) -> Result<()> {
    debug!("Writing dead node status");
    let status = NodeStatus {
        version: state.my_version,
        address: state.my_address.clone(),
        region_id: state.region_id.clone(),
        last_update: chrono::Utc::now().naive_utc(),
        state: NodeState::Dead,
        deployed_stacks: Default::default(),
    };
    write_status(state.db.as_ref(), status).await
}

async fn read_status_raw(db: &dyn DbClient, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
    db.get_raw(key).await.context("Failed to read node status")
}

fn deserialize_status(
    address_proto: protos::membership::NodeAddress,
    value_bytes: &[u8],
) -> Result<NodeStatus> {
    let status = protos::membership::NodeStatus::parse_from_bytes(value_bytes)
        .context("Failed to parse node status")?;
    NodeStatus::try_from((address_proto, status)).context("Failed to read node status")
}

#[allow(dead_code)]
async fn read_status(db: &dyn DbClient, node_port: (IpAddr, u16)) -> Result<Option<NodeStatus>> {
    let (address_proto, key) = serialize_key(node_port)?;
    let raw = read_status_raw(db, key).await?;
    raw.map(|r| deserialize_status(address_proto, r.as_ref()))
        .transpose()
}

async fn read_status_all(db: &dyn DbClient) -> Result<HashMap<(IpAddr, u16), NodeStatus>> {
    let kvs = db
        .scan_raw(DB_KEY_PREFIX.to_vec(), DB_KEY_UPPER_BOUND.to_vec(), 10240)
        .await
        .context("Failed to list existing node statuses")?;
    kvs.into_iter()
        .map(|kv| {
            let address = protos::membership::NodeAddress::parse_from_bytes(&kv.0[2..])
                .context("Failed to parse node address")?;
            let status = deserialize_status(address, kv.1.as_ref())?;
            Ok(((status.address.address, status.address.port), status))
        })
        .collect::<Result<HashMap<_, _>>>()
}

async fn write_status(db: &dyn DbClient, status: NodeStatus) -> Result<()> {
    debug!("Writing node status {status:?}");

    let (address_proto, key) = serialize_key((status.address.address, status.address.port))?;
    let my_generation = status.address.generation;
    let status_bytes = status
        .write_to_bytes()
        .context("Failed to serialize node status")?;

    loop {
        let existing_raw = read_status_raw(db, key.clone()).await?;
        let existing = existing_raw
            .map(|e| deserialize_status(address_proto.clone(), e.as_ref()).map(|x| (e, x)))
            .transpose()?;
        let written = match existing {
            Some((raw, existing)) => {
                if existing.address.generation > my_generation {
                    bail!("A newer generation of me already wrote its status, refusing to update");
                }

                db.compare_and_swap_raw(key.clone(), Some(raw), status_bytes.clone())
                    .await
                    .context("Failed to write node status")?
                    .1
            }
            None => {
                db.compare_and_swap_raw(key.clone(), None, status_bytes.clone())
                    .await
                    .context("Failed to write node status")?
                    .1
            }
        };
        if written {
            break;
        }
    }

    Ok(())
}

fn serialize_key(node_port: (IpAddr, u16)) -> Result<(protos::membership::NodeAddress, Vec<u8>)> {
    let address: protos::membership::NodeAddress = node_port.into();
    let mut key = DB_KEY_PREFIX.to_vec();
    address
        .write_to_writer(&mut key)
        .context("Failed to write key")?;
    Ok((address, key))
}

struct CompareDeployedStacksResult {
    added: Vec<StackID>,
    removed: Vec<StackID>,
}

fn compare_deployed_stack_list(
    current: &HashSet<StackID>,
    incoming: &HashSet<StackID>,
) -> CompareDeployedStacksResult {
    let mut added = vec![];
    for id in incoming {
        if !current.contains(id) {
            added.push(*id);
        }
    }

    let mut removed = vec![];
    for id in current.iter() {
        if !incoming.contains(id) {
            removed.push(*id);
        }
    }

    CompareDeployedStacksResult { added, removed }
}
