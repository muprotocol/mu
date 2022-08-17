mod node_collection;

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Display,
    net::IpAddr,
    pin::Pin,
    time::SystemTime,
};

use anyhow::{bail, Context, Error, Result};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use log::*;
use mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    ReplyChannel,
};
use rand::prelude::Distribution;
use serde::{Deserialize, Serialize};
use stable_hash::{FieldAddress, StableHash};
use tokio::{
    select,
    time::{Duration, Instant},
};
use tokio_serde::{
    formats::{Bincode, SymmetricalBincode},
    Deserializer, Serializer,
};

use crate::{network::connection_manager::ConnectionID, util::id::IdExt};

use self::node_collection::{KnownNodes, NodeCollection};

type NodeHash = u128;

/// A node in the network.
/// Assumed to run all services (executor, gateway, DB, etc.) for now.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeAddress {
    pub address: IpAddr,
    pub port: u16,
    pub generation: u128,
}

impl Display for NodeAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<Node {}:{}:{}-{}>",
            self.address,
            self.port,
            self.generation,
            self.get_hash()
        )
    }
}

impl StableHash for NodeAddress {
    fn stable_hash<H: stable_hash::StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        self.address
            .to_string()
            .stable_hash(field_address.child(0), state);
        self.port.stable_hash(field_address.child(1), state);
        self.generation.stable_hash(field_address.child(2), state);
    }
}

impl NodeAddress {
    pub fn new(address: IpAddr, port: u16) -> Self {
        let generation = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("System time cannot be before 1970-01-01")
            .unwrap()
            .as_nanos();
        Self {
            address,
            port,
            generation,
        }
    }

    pub fn get_hash(&self) -> NodeHash {
        stable_hash::fast_stable_hash(self)
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Heartbeat {
    node_address: NodeAddress,
    seq: u32,
    // The number of times this heartbeat was rebroadcast before being received
    distance: u32,
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
    Goodbye(NodeAddress),
}

#[async_trait]
pub trait Gossip {
    fn connection_available(
        &self,
        connection_request_id: ConnectionRequestID,
        connection_id: ConnectionID,
    );
    fn connection_failed(&self, connection_request_id: ConnectionRequestID, error: Error);
    fn receive_message(&self, connection_id: ConnectionID, bytes: Bytes);
    async fn get_nodes(&self) -> Result<Vec<(NodeHash, NodeAddress)>>;
    async fn stop(&self) -> Result<()>;
}

pub struct GossipConfig {
    pub heartbeat_interval: Duration,
    pub liveness_check_interval: Duration,
    pub assume_dead_after_missed_heartbeats: u32,
    pub max_peers: usize,
    pub peer_update_interval: Duration,
}

pub type NodeDiedCleanly = bool;
pub type ConnectionRequestID = u32;

enum GossipControlMessage {
    // TODO: handle network manager disconnections - note, net man should keep connections up as long as possible
    ConnectionAvailable(ConnectionRequestID, ConnectionID),
    ConnectionFailed(ConnectionRequestID, Error),

    ReceiveMessage(ConnectionID, Bytes),
    GetPeers(ReplyChannel<Vec<(NodeHash, NodeAddress)>>),
    Stop(ReplyChannel<()>),
}

pub enum GossipNotification {
    // Notifications
    NodeDiscovered(NodeAddress),
    NodeDied(NodeAddress, NodeDiedCleanly),

    // Requests
    Connect(ConnectionRequestID, IpAddr, u16),
    SendMessage(ConnectionID, Bytes),
    Disconnect(ConnectionID),
}

type NotificationChannel = mailbox_processor::NotificationChannel<GossipNotification>;

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
    my_address: NodeAddress,
    notification_channel: NotificationChannel,
    node_collection: NodeCollection,
    my_heartbeat: u32,
    codec: Codec,
    next_heartbeat: Instant,
    next_peer_update: Instant,
    next_liveness_check: Instant,
    pending_peer_connections: HashMap<ConnectionRequestID, NodeHash>,
    next_pending_peer_id: ConnectionRequestID,
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
        my_address,
        notification_channel,
        node_collection: NodeCollection::new(known_nodes),
        my_heartbeat: 0,
        codec: Bincode::default(),
        next_heartbeat: now,
        next_peer_update: now + config.peer_update_interval,
        next_liveness_check: now + config.liveness_check_interval,
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
                        info!("All senders dropped, stopping gossip");
                        break 'main_loop;
                    }

                    Some(GossipControlMessage::ConnectionAvailable(req_id, connection_id)) =>
                        process_new_connection(&mut state, req_id, connection_id),

                    Some(GossipControlMessage::ConnectionFailed(req_id, error)) =>
                        process_failed_connection(&mut state, req_id, error),

                    Some(GossipControlMessage::ReceiveMessage(id, bytes)) =>
                        if let Err(f) = receive_message(
                            id,
                            bytes,
                            &mut state
                        ) {
                            warn!("Failed to receive message: {f}");
                        },

                    Some(GossipControlMessage::GetPeers(r)) => r.reply(
                        state.node_collection
                            .get_nodes()
                            .map(|(k, v)| (*k, v.address.clone()))
                            .collect()
                    ),

                    Some(GossipControlMessage::Stop(r)) => {
                        if let Err(f) = send_goodbye(&mut state) {
                            error!("Failed to send goodbye: {}", f);
                        }
                        r.reply(());
                        break 'main_loop;
                    }
                }
            }
        }
    }
}

fn send_heartbeat(state: &mut GossipState) -> Result<()> {
    state.my_heartbeat += 1;

    debug!("Sending heartbeat #{}", state.my_heartbeat);

    let message = GossipProtocolMessage::Heartbeat(Heartbeat {
        node_address: state.my_address.clone(),
        seq: state.my_heartbeat,
        distance: 1,
    });

    send_protocol_message(message, state, vec![])
}

fn send_goodbye(state: &mut GossipState) -> Result<()> {
    debug!("Sending goodbye");

    let message = GossipProtocolMessage::Goodbye(state.my_address.clone());

    send_protocol_message(message, state, vec![])
}

fn send_protocol_message(
    message: GossipProtocolMessage,
    state: &mut GossipState,
    excluded_peers: Vec<NodeHash>,
) -> Result<()> {
    debug!("Sending protocol message {message:?} to all except {excluded_peers:?}");

    let message_bytes = Pin::new(&mut state.codec)
        .serialize(&message)
        .context("Failed to serialize protocol message")?;

    for (hash, _, peer) in state.node_collection.get_peers_raw() {
        if !excluded_peers.contains(&hash) {
            debug!("Sending protocol message {message:?} to {hash}");
            state
                .notification_channel
                .send(GossipNotification::SendMessage(
                    peer.connection_id,
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
    debug!("Received protocol message: {message:?}");
    match message {
        GossipProtocolMessage::Heartbeat(heartbeat) => {
            let hash = heartbeat.node_address.get_hash();
            let mut seen = false;
            match state.nodes.entry(hash) {
                Entry::Occupied(mut entry) => {
                    debug!(
                        "Heartbeat #{} from known node {}",
                        heartbeat.seq, heartbeat.node_address
                    );

                    let mut node = entry.get_mut();

                    if node.last_heartbeat < heartbeat.seq {
                        node.last_heartbeat = heartbeat.seq;
                        node.last_heartbeat_timestamp = Instant::now();
                    } else {
                        seen = true;
                    }

                    if node.distance > heartbeat.distance // Shorter path discovered...
                        // ... or a node along shortest path died
                        || heartbeat.seq - node.distance_seq > state.config.assume_dead_after_missed_heartbeats
                    {
                        node.distance = heartbeat.distance;
                        node.distance_seq = heartbeat.seq;
                    } else if node.distance == heartbeat.distance {
                        node.distance_seq = heartbeat.seq;
                    }

                    if node.distance == 1 && !state.peers.contains(&hash) {
                        node.peer_connection = Some(PeerConnection {
                            connection_id,
                            is_temporary: false,
                        });
                        state.peers.insert(hash);
                    }

                    // TODO: this will cause missed messages when a peer disconnects and
                    // then connects again later. We have no way around it anyway, since
                    // a new connection can't immediately be identified and must present
                    // its node information. This will be fixed when reconnections are
                    // implemented in the connection manager.
                    if let Some(peer) = node.peer_connection.as_mut() {
                        if peer.connection_id != connection_id {
                            debug!(
                                "Peer {} was reconnected and now has connection ID {}",
                                hash, peer.connection_id
                            );
                            peer.connection_id = connection_id;
                        }
                    }
                }

                Entry::Vacant(e) => {
                    // TODO: check for existing older generation of same node

                    debug!(
                        "Heartbeat #{} from new node {}",
                        heartbeat.seq, heartbeat.node_address
                    );

                    let node = Node::from_heartbeat(&heartbeat);

                    let node = e.insert(node);
                    state
                        .notification_channel
                        .send(GossipNotification::NodeDiscovered(node.address.clone()));
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
                    error!("Failed to replicate heartbeat due to {f}");
                }
            }
        }

        GossipProtocolMessage::Goodbye(node_address) => {
            let hash = node_address.get_hash();
            match state.nodes.remove(&hash) {
                Some(node) => {
                    state.peers.remove(&hash);
                    debug!("Goodbye from node {}", node.address);
                    state
                        .notification_channel
                        .send(GossipNotification::NodeDied(node.address, true));

                    if let Err(f) = send_protocol_message(
                        GossipProtocolMessage::Goodbye(node_address),
                        state,
                        vec![], // We already removed the peer, so no need to filter again
                    ) {
                        error!("Failed to replicate heartbeat due to {f}");
                    }
                }

                None => (), // Goodbyes are replicated, so we may get them many times
            }
        }
    }

    Ok(())
}

async fn perform_maintenance(state: &mut GossipState) -> Instant {
    let now = Instant::now();

    if almost_at_or_after_instant(now, state.next_heartbeat) {
        if let Err(f) = send_heartbeat(state) {
            error!("Failed to send heartbeat: {f}");
        }
        state.next_heartbeat = state.next_heartbeat + state.config.heartbeat_interval;
    }

    if almost_at_or_after_instant(now, state.next_liveness_check) {
        if let Err(f) = perform_liveness_check(state, now) {
            error!("Failed to send heartbeat: {f}");
        }
        state.next_liveness_check =
            state.next_liveness_check + state.config.liveness_check_interval;
    }

    if almost_at_or_after_instant(now, state.next_peer_update) {
        if let Err(f) = perform_peer_update(state) {
            error!("Failed to send heartbeat: {f}");
        }
        state.next_peer_update = state.next_peer_update + state.config.peer_update_interval;
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
    debug!("Performing liveness checks");

    let assume_dead_duration =
        state.config.heartbeat_interval * state.config.assume_dead_after_missed_heartbeats;

    let mut dead = vec![];

    for (hash, node) in &state.nodes {
        let since_last_heartbeat = now.saturating_duration_since(node.last_heartbeat_timestamp);
        if since_last_heartbeat > assume_dead_duration
            // This scans the entire map, but there will always only be a few entries.
            // If we're attempting to connect to a node, we should wait until we're sure
            // the connection couldn't be established
            && !state.pending_peer_connections.values().any(|x| *x == *hash)
        {
            debug!("No heartbeat from node {} for {since_last_heartbeat:?}, assuming dead and removing from known nodes", node.address);
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
            .send(GossipNotification::NodeDied(node.address.clone(), false));
    }

    Ok(())
}

fn perform_peer_update(state: &mut GossipState) -> Result<()> {
    debug!("Performing peer update");

    let mut rng = rand::thread_rng();

    let non_temp_peer_count = get_peer_nodes(state)
        .filter(|(_, _, p)| !p.is_temporary)
        .count();

    if non_temp_peer_count < state.config.max_peers {
        debug!("Too few peers, promoting a node");

        fn is_candidate(n: &Node) -> bool {
            match n.peer_connection {
                Some(PeerConnection {
                    is_temporary: true, ..
                })
                | None => true,
                _ => false,
            }
        }

        let dist = match rand::distributions::weighted::WeightedIndex::new(
            state
                .nodes
                .iter()
                .filter(|n| is_candidate(n.1))
                .map(|n| n.1.distance + 1),
        ) {
            Ok(d) => d,
            Err(_) => {
                // This happens when we give WeightedIndex zero weights to work with
                debug!("No nodes to promote");
                return Ok(());
            }
        };

        let index = dist.sample(&mut rng);
        let (hash, node) = match state
            .nodes
            .iter_mut()
            .filter(|n| is_candidate(n.1))
            .skip(index)
            .next()
        {
            None => {
                bail!(
                    "Failed to get node at random index {index}, don't have enough non-peer nodes"
                )
            }
            Some(n) => n,
        };

        match node.peer_connection.as_mut() {
            Some(peer_connection) => {
                // Already have an active connection to this node, just mark it as not temporary
                peer_connection.is_temporary = false;
            }
            None => {
                let req_id = state.next_pending_peer_id.get_and_increment();
                state.pending_peer_connections.insert(req_id, *hash);
                state.notification_channel.send(GossipNotification::Connect(
                    req_id,
                    node.address.address,
                    node.address.port,
                ));
            }
        }
    }

    let peer_count = state.peers.iter().count();

    if peer_count > state.config.max_peers {
        let temp_peers = get_peer_nodes(state)
            .filter(|(_, _, p)| p.is_temporary)
            .collect::<Vec<_>>();
        if temp_peers.len() > 0 {
            let index = rand::distributions::Uniform::new(0, temp_peers.len()).sample(&mut rng);
            let (hash, _, _) = temp_peers[index];
            disconnect(state, hash);
        }
    }

    Ok(())
}

fn disconnect(state: &mut GossipState, hash: u128) -> Option<Node> {
    if let Some(node) = state.nodes.remove(&hash) {
        state.peers.remove(&hash);

        if let Some(peer) = &node.peer_connection {
            state
                .notification_channel
                .send(GossipNotification::Disconnect(peer.connection_id));
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
) {
    if let Some(hash) = state.pending_peer_connections.remove(&req_id) {
        if let Some(node) = state.nodes.get_mut(&hash) {
            if let Some(peer) = &node.peer_connection {
                warn!("Received connection ID {connection_id} for node {hash} with existing peer connection {}", peer.connection_id);
                state
                    .notification_channel
                    .send(GossipNotification::Disconnect(connection_id));
                return;
            }

            node.peer_connection = Some(PeerConnection {
                connection_id,
                is_temporary: false,
            });
            state.peers.insert(hash);
        }
    }
}

fn process_failed_connection(state: &mut GossipState, req_id: ConnectionRequestID, error: Error) {
    // Simply remove the pending connection, the peer update routine will create another one
    // TODO: speed up the next peer update?
    if let Some(hash) = state.pending_peer_connections.remove(&req_id) {
        warn!("Failed to connect to node {hash} due to {error}");
    }
}
