mod node_collection;

use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    net::IpAddr,
    pin::Pin,
    time::Duration,
};

use anyhow::{bail, Context, Error, Result};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use dyn_clonable::clonable;
use mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    ReplyChannel,
};
use mu_stack::StackID;
use rand::{prelude::Distribution, rngs::ThreadRng};
use serde::{Deserialize, Serialize};
use tokio::{select, time::Instant};
use tokio_serde::{
    formats::{Bincode, SymmetricalBincode},
    Deserializer, Serializer,
};

use crate::{infrastructure::config::ConfigDuration, util::id::IdExt};

pub use self::node_collection::KnownNodes;
use self::node_collection::*;

use super::{ConnectionID, NodeAddress, NodeConnection, NodeHash};

macro_rules! debug {
    ($state:expr, $($arg:tt)+) => (log::debug!(target: &$state.log_target, $($arg)+))
}

macro_rules! info {
    ($state:expr, $($arg:tt)+) => (log::info!(target: &$state.log_target, $($arg)+))
}

macro_rules! warn {
    ($state:expr, $($arg:tt)+) => (log::warn!(target: &$state.log_target, $($arg)+))
}

macro_rules! error {
    ($state:expr, $($arg:tt)+) => (log::error!(target: &$state.log_target, $($arg)+))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Heartbeat {
    node_address: NodeAddress,
    seq: u32,
    // The number of times this heartbeat was rebroadcast before being received
    distance: u32,
    // TODO: add number of known nodes to heartbeats, so we can have a general idea of
    // how "connected" we are.
    deployed_stacks: Vec<StackID>,
}

// TODO: replace with version-tolerant solution
#[derive(Serialize, Deserialize, Clone, Debug)]
enum GossipProtocolMessage {
    /// Each node sends out heartbeat messages to peers at regular intervals.
    /// Peers then rebroadcast the heartbeat to their own peers.
    Heartbeat(Heartbeat),

    /// Each node sends a Goodbye message when shutting down cleanly.
    /// This helps other nodes maintain an up-to-date state of the network.
    /// Nodes propagate Goodbye messages similarly to Hello messages.
    // TODO: transmit goodbye messages repeatedly for a while to
    // let more nodes get them
    Goodbye(NodeAddress),
}

#[async_trait]
#[clonable]
pub trait Gossip: Clone + Sync + Send {
    fn connection_available(
        &self,
        connection_request_id: ConnectionRequestID,
        connection_id: ConnectionID,
    );
    fn connection_failed(&self, connection_request_id: ConnectionRequestID, error: Error);
    fn receive_message(&self, connection_id: ConnectionID, bytes: Bytes);
    async fn get_nodes(&self) -> Result<Vec<(NodeHash, NodeAddress)>>;
    async fn stop(&self) -> Result<()>;

    async fn stack_deployed_locally(&self, stack_id: StackID) -> Result<()>;
    async fn stack_undeployed_locally(&self, stack_id: StackID) -> Result<()>;

    async fn get_connection(&self, hash: NodeHash) -> Result<Option<NodeConnection>>;

    #[cfg(debug_assertions)]
    async fn log_statistics(&self);
}

#[derive(Clone, Deserialize, Debug)]
pub struct GossipConfig {
    pub heartbeat_interval: ConfigDuration,
    pub liveness_check_interval: ConfigDuration,
    pub assume_dead_after_missed_heartbeats: u32,
    pub max_peers: usize,
    pub peer_update_interval: ConfigDuration,
}

#[derive(Deserialize)]
pub struct KnownNodeConfig {
    pub ip: IpAddr,
    pub gossip_port: u16,
    pub pd_port: u16,
}

pub type NodeDiedCleanly = bool;
pub type ConnectionRequestID = u32;

enum GossipControlMessage {
    // TODO: handle network manager disconnections - note, net man should keep connections up as long as possible
    ConnectionAvailable(ConnectionRequestID, ConnectionID),
    ConnectionFailed(ConnectionRequestID, Error),

    StackDeployedLocally(StackID, ReplyChannel<()>),
    StackUndeployedLocally(StackID, ReplyChannel<()>),

    ReceiveMessage(ConnectionID, Bytes),
    GetPeers(ReplyChannel<Vec<(NodeHash, NodeAddress)>>),
    Stop(ReplyChannel<()>),

    GetConnection(NodeHash, ReplyChannel<Option<NodeConnection>>),

    #[cfg(debug_assertions)]
    LogStatistics(ReplyChannel<()>),
}

#[derive(Debug)]
pub enum GossipNotification {
    // Notifications
    NodeDiscovered(NodeAddress),
    NodeDied(NodeAddress, NodeDiedCleanly),
    NodeDeployedStacks(NodeAddress, Vec<StackID>), // TODO
    NodeUndeployedStacks(NodeAddress, Vec<StackID>), // TODO

    // Requests
    Connect(ConnectionRequestID, IpAddr, u16),
    SendMessage(ConnectionID, Bytes),
    Disconnect(ConnectionID),
}

type NotificationChannel = mailbox_processor::NotificationChannel<GossipNotification>;

#[derive(Clone)]
struct GossipImpl {
    mailbox: PlainMailboxProcessor<GossipControlMessage>,
}

#[async_trait]
impl Gossip for GossipImpl {
    fn connection_available(
        &self,
        connection_request_id: ConnectionRequestID,
        connection_id: ConnectionID,
    ) {
        self.mailbox
            .post_and_forget(GossipControlMessage::ConnectionAvailable(
                connection_request_id,
                connection_id,
            ));
    }

    fn connection_failed(&self, connection_request_id: ConnectionRequestID, error: Error) {
        self.mailbox
            .post_and_forget(GossipControlMessage::ConnectionFailed(
                connection_request_id,
                error,
            ));
    }

    fn receive_message(&self, connection_id: ConnectionID, bytes: Bytes) {
        self.mailbox
            .post_and_forget(GossipControlMessage::ReceiveMessage(connection_id, bytes));
    }

    async fn get_nodes(&self) -> Result<Vec<(NodeHash, NodeAddress)>> {
        self.mailbox
            .post_and_reply(GossipControlMessage::GetPeers)
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox
            .post_and_reply(GossipControlMessage::Stop)
            .await
            .map_err(Into::into)
    }

    async fn stack_deployed_locally(&self, stack_id: StackID) -> Result<()> {
        self.mailbox
            .post_and_reply(|r| GossipControlMessage::StackDeployedLocally(stack_id, r))
            .await
            .map_err(Into::into)
    }

    async fn stack_undeployed_locally(&self, stack_id: StackID) -> Result<()> {
        self.mailbox
            .post_and_reply(|r| GossipControlMessage::StackUndeployedLocally(stack_id, r))
            .await
            .map_err(Into::into)
    }

    async fn get_connection(&self, hash: NodeHash) -> Result<Option<NodeConnection>> {
        self.mailbox
            .post_and_reply(|r| GossipControlMessage::GetConnection(hash, r))
            .await
            .map_err(Into::into)
    }

    #[cfg(debug_assertions)]
    async fn log_statistics(&self) {
        self.mailbox
            .post_and_reply(GossipControlMessage::LogStatistics)
            .await
            .unwrap()
    }
}

type Codec = SymmetricalBincode<GossipProtocolMessage>;

pub fn start(
    my_address: NodeAddress,
    config: GossipConfig,
    known_nodes: KnownNodes,
    notification_channel: NotificationChannel,
) -> Result<Box<dyn Gossip>> {
    let mailbox = PlainMailboxProcessor::start(
        move |_mb, r| body(r, my_address, config, known_nodes, notification_channel),
        10000,
    );

    Ok(Box::new(GossipImpl { mailbox }))
}

struct GossipState {
    config: GossipConfig,
    notification_channel: NotificationChannel,
    node_collection: NodeCollection,

    my_address: NodeAddress,
    my_heartbeat: u32,
    deployed_stacks: HashSet<StackID>,
    codec: Codec,

    // maintenance-related fields
    next_heartbeat: Instant,
    next_peer_update: Instant,
    next_liveness_check: Instant,

    // pending connections
    pending_peer_connections: HashMap<ConnectionRequestID, NodeHash>,
    next_pending_peer_id: ConnectionRequestID,

    log_target: String,
}

#[cfg(debug_assertions)]
fn log_target(port: u16) -> String {
    format!("{}::{}", module_path!(), port)
}

#[cfg(not(debug_assertions))]
fn log_target(_port: u16) -> String {
    module_path!().to_string()
}

async fn body(
    mut message_receiver: MessageReceiver<GossipControlMessage>,
    my_address: NodeAddress,
    config: GossipConfig,
    known_nodes: KnownNodes,
    notification_channel: NotificationChannel,
) {
    let now = Instant::now();

    let mut state = GossipState {
        log_target: log_target(my_address.port),
        notification_channel,
        node_collection: NodeCollection::new(known_nodes),
        my_address,
        my_heartbeat: 0,
        deployed_stacks: HashSet::new(),
        codec: Bincode::default(),
        next_heartbeat: now,
        next_peer_update: now + *config.peer_update_interval,
        next_liveness_check: now + *config.liveness_check_interval,
        pending_peer_connections: HashMap::new(),
        next_pending_peer_id: 0,
        config,
    };

    let next_maintenance = perform_maintenance(&mut state).await;

    let mut maintenance_timeout = Box::pin(tokio::time::sleep_until(next_maintenance));

    'main_loop: loop {
        select! {
            // This also handles immediately sending heartbeats to known nodes,
            // since the timer ticks once immediately
            _ = maintenance_timeout.as_mut() => {
                let next_maintenance = perform_maintenance(&mut state).await;
                maintenance_timeout.as_mut().reset(next_maintenance);
            }

            msg = message_receiver.receive() => {
                match msg {
                    None => {
                        info!(state, "All senders dropped, stopping gossip");
                        break 'main_loop;
                    }

                    Some(GossipControlMessage::ConnectionAvailable(req_id, connection_id)) => {
                        if let Err(f) = process_new_connection(&mut state, req_id, connection_id) {
                            warn!(state, "Failed to process new connection: {f}");
                        }
                    }

                    Some(GossipControlMessage::ConnectionFailed(req_id, error)) =>
                        process_failed_connection(&mut state, req_id, error),

                    Some(GossipControlMessage::ReceiveMessage(id, bytes)) =>
                        if let Err(f) = receive_message(
                            id,
                            bytes,
                            &mut state
                        ) {
                            warn!(state, "Failed to receive message: {f}");
                        },

                    Some(GossipControlMessage::GetPeers(r)) => r.reply(
                        state.node_collection
                            .get_nodes_and_hashes()
                            // We don't report nodes with generation 0, since we haven't
                            // received a heartbeat from those nodes yet.
                            .filter_map(|(hash, node)| {
                                let info = node.info();
                                if info.address.generation == 0 {
                                    None
                                } else {
                                    Some((*hash, node.info().address.clone()))
                                }
                            })
                            .collect()
                    ),

                    Some(GossipControlMessage::Stop(r)) => {
                        if let Err(f) = send_goodbye(&mut state) {
                            error!(state, "Failed to send goodbye: {}", f);
                        }
                        r.reply(());
                        break 'main_loop;
                    },

                    Some(GossipControlMessage::StackDeployedLocally(stack_id, r)) => {
                        state.deployed_stacks.insert(stack_id);
                        r.reply(());
                    },

                    Some(GossipControlMessage::StackUndeployedLocally(stack_id, r)) => {
                        state.deployed_stacks.remove(&stack_id);
                        r.reply(());
                    },

                    Some(GossipControlMessage::GetConnection(node_hash, r)) => {
                        r.reply(get_connection(&state, &node_hash));
                    }

                    #[cfg(debug_assertions)]
                    Some(GossipControlMessage::LogStatistics(r)) => {
                        log_statistics(&state);
                        r.reply(());
                    }
                }
            }
        }
    }
}

fn get_connection(state: &GossipState, node_hash: &NodeHash) -> Option<NodeConnection> {
    state.node_collection.get_node(node_hash).map(|n| match n {
        Node::RemoteNode(remote) => NodeConnection::NotEstablished(remote.info().address.clone()),
        Node::Peer(peer) => NodeConnection::Established(peer.connection_id()),
    })
}

fn send_heartbeat(state: &mut GossipState) -> Result<()> {
    state.my_heartbeat += 1;

    debug!(state, "Sending heartbeat #{}", state.my_heartbeat);

    let message = GossipProtocolMessage::Heartbeat(Heartbeat {
        node_address: state.my_address.clone(),
        seq: state.my_heartbeat,
        distance: 1,
        deployed_stacks: state.deployed_stacks.iter().cloned().collect(),
    });

    send_protocol_message(message, state, vec![])
}

fn send_goodbye(state: &mut GossipState) -> Result<()> {
    debug!(state, "Sending goodbye");

    let message = GossipProtocolMessage::Goodbye(state.my_address.clone());

    send_protocol_message(message, state, vec![])
}

fn send_protocol_message(
    message: GossipProtocolMessage,
    state: &mut GossipState,
    excluded_peers: Vec<NodeHash>,
) -> Result<()> {
    debug!(
        state,
        "Sending protocol message {message:?} to all except {excluded_peers:?}"
    );

    let message_bytes = Pin::new(&mut state.codec)
        .serialize(&message)
        .context("Failed to serialize protocol message")?;

    for (hash, peer) in state.node_collection.get_peers_and_hashes() {
        if !excluded_peers.contains(hash) {
            debug!(state, "Sending protocol message {message:?} to {hash}");
            state
                .notification_channel
                .send(GossipNotification::SendMessage(
                    peer.connection_id(),
                    message_bytes.clone(),
                ));
        }
    }

    Ok(())
}

fn receive_message(
    connection_id: ConnectionID,
    bytes: Bytes,
    state: &mut GossipState,
) -> Result<()> {
    // TODO: why does deserialize take a BytesMut? Is there a way to deserialize from Bytes directly?
    let buf: &[u8] = &bytes;
    let bytes_mut: BytesMut = buf.into();
    let message = Pin::new(&mut state.codec)
        .deserialize(&bytes_mut)
        .context("Failed to deserialize message")?;
    debug!(state, "Received protocol message: {message:?}");
    match message {
        GossipProtocolMessage::Heartbeat(heartbeat) => {
            let mut seen = false;
            let hash = heartbeat.node_address.get_hash();

            match state.node_collection.node_entry(&heartbeat.node_address) {
                NodeEntry::Occupied(occ) => {
                    debug!(
                        state,
                        "Heartbeat #{} from known node {}", heartbeat.seq, heartbeat.node_address
                    );

                    // Step 1: sync generations
                    let mut same_generation = match occ {
                        OccupiedByGeneration::Same(same) => same,
                        OccupiedByGeneration::Older(old) => {
                            let old_address = &old.get().info().address;
                            debug!(
                                state,
                                "Discovered newer generation {} of peer {}",
                                heartbeat.node_address.generation,
                                old_address
                            );
                            // If the old address has generation 0, this is actually the
                            // first heartbeat we're receiving from this node
                            if old_address.generation > 0 {
                                state
                                    .notification_channel
                                    .send(GossipNotification::NodeDied(old_address.clone(), true))
                            };
                            state
                                .notification_channel
                                .send(GossipNotification::NodeDiscovered(
                                    heartbeat.node_address.clone(),
                                ));
                            old.update_generation(heartbeat.node_address.generation)
                        }
                        OccupiedByGeneration::Newer() => {
                            debug!(
                                state,
                                "Already know newer version of node {}, ignoring heartbeat",
                                heartbeat.node_address
                            );
                            return Ok(());
                        }
                    };

                    // Step 2: update heartbeat seq and distance
                    let node = same_generation.get_mut();
                    let info = node.info_mut();

                    if info.last_heartbeat >= heartbeat.seq {
                        debug!(
                            state,
                            "Already seen heartbeat {} from {}, won't process",
                            info.last_heartbeat,
                            heartbeat.node_address
                        );
                        return Ok(());
                    }

                    if info.last_heartbeat < heartbeat.seq {
                        info.last_heartbeat = heartbeat.seq;
                        info.last_heartbeat_timestamp = Instant::now();
                    } else {
                        seen = true;
                    }

                    if info.distance > heartbeat.distance // Shorter path discovered...
                        // ... or a node along shortest path died
                        || heartbeat.seq - info.distance_seq > state.config.assume_dead_after_missed_heartbeats
                    {
                        info.distance = heartbeat.distance;
                        info.distance_seq = heartbeat.seq;
                    } else if info.distance == heartbeat.distance {
                        info.distance_seq = heartbeat.seq;
                    }

                    // Step 3: sync deployed stacks
                    let CompareDeployedStacksResult { added, removed } =
                        compare_deployed_stack_list(
                            &mut info.deployed_stacks,
                            &heartbeat.deployed_stacks,
                        );

                    if !added.is_empty() {
                        state
                            .notification_channel
                            .send(GossipNotification::NodeDeployedStacks(
                                heartbeat.node_address.clone(),
                                added,
                            ));
                    }

                    if !removed.is_empty() {
                        state
                            .notification_channel
                            .send(GossipNotification::NodeUndeployedStacks(
                                heartbeat.node_address.clone(),
                                removed,
                            ));
                    }

                    if heartbeat.distance == 1 {
                        // Step 4: Promote to peer if distance is 1 (i.e. the other node
                        // chose us as a peer)
                        if let HashEntry::OccupiedBySameGeneration(mut occ) =
                            state.node_collection.hash_entry(&hash)
                        {
                            if let Node::RemoteNode(_) = occ.get() {
                                occ.promote_to_peer(connection_id)
                                    .context("Failed to promote node to peer")?;
                            }
                        }

                        // Step 5: update connection ID for newly reconnected peers

                        // TODO: this will cause missed messages when a peer disconnects and
                        // then connects again later. We have no way around it anyway, since
                        // a new connection can't immediately be identified and must present
                        // its node information. This will be fixed when reconnections are
                        // implemented in the connection manager.
                        if let Some(Node::Peer(peer)) = state.node_collection.get_node_mut(&hash) {
                            if peer.connection_id() != connection_id {
                                debug!(
                                    state,
                                    "Peer {} was reconnected and now has connection ID {}",
                                    hash,
                                    connection_id
                                );
                                peer.set_connection_id(connection_id);
                            }
                        }
                    }
                }

                NodeEntry::Vacant(vac) => {
                    debug!(
                        state,
                        "Heartbeat #{} from new node {}", heartbeat.seq, heartbeat.node_address
                    );

                    let info = NodeInfo::from_heartbeat(&heartbeat);

                    let info = vac.insert_remote(info).info();

                    state
                        .notification_channel
                        .send(GossipNotification::NodeDiscovered(info.address.clone()));

                    if !info.deployed_stacks.is_empty() {
                        state
                            .notification_channel
                            .send(GossipNotification::NodeDeployedStacks(
                                heartbeat.node_address.clone(),
                                info.deployed_stacks.iter().cloned().collect(),
                            ));
                    }
                }
            }

            if !seen {
                let new_heartbeat = Heartbeat {
                    distance: heartbeat.distance + 1,
                    ..heartbeat
                };

                if let Err(f) = send_protocol_message(
                    GossipProtocolMessage::Heartbeat(new_heartbeat),
                    state,
                    vec![hash],
                ) {
                    error!(state, "Failed to replicate heartbeat due to {f}");
                }
            }
        }

        GossipProtocolMessage::Goodbye(node_address) => {
            let hash = node_address.get_hash();
            if let Some(node) = state.node_collection.remove(&hash) {
                // Goodbyes are replicated, so we may get them many times
                let info = node.into_info();

                debug!(state, "Goodbye from node {}", info.address);

                state
                    .notification_channel
                    .send(GossipNotification::NodeDied(info.address, true));

                if let Err(f) = send_protocol_message(
                    GossipProtocolMessage::Goodbye(node_address),
                    state,
                    vec![], // We already removed the peer, so no need to filter again
                ) {
                    error!(state, "Failed to replicate goodbye due to {f}");
                }
            }
        }
    }

    Ok(())
}

struct CompareDeployedStacksResult {
    added: Vec<StackID>,
    removed: Vec<StackID>,
}

fn compare_deployed_stack_list(
    current: &mut HashSet<StackID>,
    incoming: &Vec<StackID>,
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

    for id in &added {
        current.insert(*id);
    }

    for id in &removed {
        current.remove(id);
    }

    CompareDeployedStacksResult { added, removed }
}

async fn perform_maintenance(state: &mut GossipState) -> Instant {
    let now = Instant::now();

    if almost_at_or_after_instant(now, state.next_heartbeat) {
        if let Err(f) = send_heartbeat(state) {
            error!(state, "Failed to send heartbeat: {f}");
        }
        state.next_heartbeat += *state.config.heartbeat_interval;
    }

    if almost_at_or_after_instant(now, state.next_liveness_check) {
        if let Err(f) = perform_liveness_check(state, now) {
            error!(state, "Failed to send heartbeat: {f}");
        }
        state.next_liveness_check += *state.config.liveness_check_interval;
    }

    if almost_at_or_after_instant(now, state.next_peer_update) {
        if let Err(f) = perform_peer_update(state) {
            error!(state, "Failed to send heartbeat: {f}");
        }
        state.next_peer_update += *state.config.peer_update_interval;
    }

    state
        .next_heartbeat
        .min(state.next_liveness_check)
        .min(state.next_peer_update)
}

fn almost_at_or_after_instant(now: Instant, target: Instant) -> bool {
    const MAX_ERROR: Duration = Duration::from_millis(10);
    target.saturating_duration_since(now) < MAX_ERROR
}

fn perform_liveness_check(state: &mut GossipState, now: Instant) -> Result<()> {
    debug!(state, "Performing liveness checks");

    let assume_dead_duration =
        *state.config.heartbeat_interval * state.config.assume_dead_after_missed_heartbeats;

    let mut dead = vec![];

    for (hash, node) in state.node_collection.get_nodes_and_hashes() {
        let info = node.info();
        let since_last_heartbeat = now.saturating_duration_since(info.last_heartbeat_timestamp);
        if since_last_heartbeat > assume_dead_duration
            // This scans the entire map, but there will always only be a few entries.
            // If we're attempting to connect to a node, we should wait until we're sure
            // the connection couldn't be established
            && !state.pending_peer_connections.values().any(|x| *x == *hash)
        {
            debug!(state, "No heartbeat from node {} for {since_last_heartbeat:?}, assuming dead and removing from known nodes", info.address);
            dead.push(*hash);
        }
    }

    for hash in dead {
        // If a peer is dead, we should disconnect from it.
        // This gets even more important once connection manager
        // starts reconnecting dropped connections.
        let node = disconnect(state, hash)
            .context("Node was just seen")
            .unwrap();

        state
            .notification_channel
            .send(GossipNotification::NodeDied(
                node.info().address.clone(),
                false,
            ));
    }

    Ok(())
}

fn perform_peer_update(state: &mut GossipState) -> Result<()> {
    debug!(state, "Performing peer update");

    let mut rng = rand::thread_rng();

    let permanent_peer_count = state
        .node_collection
        .get_permanent_peers_and_hashes()
        .count();

    if permanent_peer_count < state.config.max_peers {
        debug!(state, "Too few peers, promoting a node");
        promote_random_to_permanent_peer(state, &mut rng)
            .context("Failed to promote a node to peer")?;
    }

    let peer_count = state.node_collection.get_peers().count();

    // Other nodes may choose this node as a peer more times than we want,
    // in which case we attempt to remove temporary peer connections. If
    // there are no temporary connections, we just ignore the extra peers.
    if peer_count > state.config.max_peers {
        let temp_peers = state
            .node_collection
            .get_temporary_peers_and_hashes()
            .collect::<Vec<_>>();
        if !temp_peers.is_empty() {
            debug!(state, "Too many peers, dropping a temporary");
            let index = rand::distributions::Uniform::new(0, temp_peers.len()).sample(&mut rng);
            let (hash, _) = temp_peers[index];
            disconnect(state, *hash);
        }
    }

    Ok(())
}

fn promote_random_to_permanent_peer(state: &mut GossipState, rng: &mut ThreadRng) -> Result<()> {
    fn is_promotion_candidate(n: &Node) -> bool {
        match n {
            Node::RemoteNode(_) => true,
            Node::Peer(Peer::Temporary(_)) => true,
            Node::Peer(Peer::Permanent(_)) => false,
        }
    }

    let dist = match rand::distributions::weighted::WeightedIndex::new(
        state
            .node_collection
            .get_nodes()
            .filter(|n| is_promotion_candidate(n))
            .map(|n| n.info().distance + 1),
    ) {
        Ok(d) => d,
        Err(_) => {
            // This happens when we give WeightedIndex zero weights to work with
            debug!(state, "No nodes to promote");
            return Ok(());
        }
    };

    let index = dist.sample(rng);

    let (hash, _) = match state
        .node_collection
        .get_nodes_and_hashes()
        .filter(|n| is_promotion_candidate(n.1))
        .nth(index)
    {
        None => {
            bail!("Failed to get node at random index {index}, don't have enough non-peer nodes")
        }
        Some(n) => n,
    };
    let hash = *hash;

    match state.node_collection.hash_entry(&hash) {
        HashEntry::OccupiedBySameGeneration(mut occ) => match occ.get() {
            node @ Node::RemoteNode(_) => {
                // We must first establish a connection to the remote node before we can make it a peer
                let req_id = state.next_pending_peer_id.get_and_increment();
                state.pending_peer_connections.insert(req_id, hash);

                let address = node.info().address.clone();
                state.notification_channel.send(GossipNotification::Connect(
                    req_id,
                    address.address,
                    address.port,
                ));
            }
            Node::Peer(Peer::Temporary(_)) => {
                occ.promote_to_permanent()
                    .context("Failed to promote known temporary peer to permanent")?;
            }
            Node::Peer(Peer::Permanent(_)) => {
                panic!("Impossible, already filtered out permanent peers above")
            }
        },
        HashEntry::Vacant(_) => panic!("Impossible, node was already seen above"),
    }

    Ok(())
}

fn disconnect(state: &mut GossipState, hash: NodeHash) -> Option<Node> {
    if let Some(node) = state.node_collection.remove(&hash) {
        if let Node::Peer(peer) = &node {
            state
                .notification_channel
                .send(GossipNotification::Disconnect(peer.connection_id()));
        }

        Some(node)
    } else {
        None
    }
}

fn process_new_connection(
    state: &mut GossipState,
    req_id: ConnectionRequestID,
    connection_id: ConnectionID,
) -> Result<()> {
    if let Some(hash) = state.pending_peer_connections.remove(&req_id) {
        match state.node_collection.hash_entry(&hash) {
            HashEntry::OccupiedBySameGeneration(mut occ) => match occ.get() {
                Node::Peer(peer) => {
                    warn!(state, "Received connection ID {connection_id} for node {hash} with existing peer connection {}", peer.connection_id());
                    state
                        .notification_channel
                        .send(GossipNotification::Disconnect(connection_id));
                }
                node @ Node::RemoteNode(_) => {
                    debug!(
                        state,
                        "Promoting {} to peer with connection ID {connection_id}",
                        node.info().address
                    );
                    occ.promote_to_peer(connection_id)
                        .context("Failed to promote known remote node to peer")?;
                }
            },

            HashEntry::Vacant(_) => {
                warn!(
                    state,
                    "Received connection ID {connection_id} for unknown node {hash}"
                );
                state
                    .notification_channel
                    .send(GossipNotification::Disconnect(connection_id));
            }
        }
    } else {
        bail!("New connection {connection_id} for unknown connection request {req_id}");
    }

    Ok(())
}

fn process_failed_connection(state: &mut GossipState, req_id: ConnectionRequestID, error: Error) {
    // Simply remove the pending connection, the peer update routine will create another one
    // TODO: speed up the next peer update?
    if let Some(hash) = state.pending_peer_connections.remove(&req_id) {
        warn!(state, "Failed to connect to node {hash} due to {error}");
    }
}

#[cfg(debug_assertions)]
fn log_statistics(state: &GossipState) {
    debug!(state, "#############################################");
    let mut nodes = state.node_collection.get_nodes().collect::<Vec<_>>();
    debug!(state, "Known node count: {}", nodes.len());
    debug!(
        state,
        "Peer count: {}",
        state.node_collection.get_peers().count()
    );
    nodes.sort_unstable_by(|a, b| match (a, b) {
        (Node::Peer(_), Node::RemoteNode(_)) => std::cmp::Ordering::Greater,
        (Node::RemoteNode(_), Node::Peer(_)) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    });
    for (i, node) in nodes.iter().enumerate() {
        debug!(state, "Node {i} is: {node:?}");
    }
    debug!(state, "#############################################");
}
