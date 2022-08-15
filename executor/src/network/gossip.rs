// TODO:
// * Implement heartbeat propagation from seed nodes (subject to discussion with @thepeak)
//   * Nodes should try to connect to peers they don't have an active connection to
//   * Drop one of the connections if we get two due to retries

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Display,
    net::IpAddr,
    pin::Pin,
    time::SystemTime,
};

use anyhow::{Context, Ok, Result};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use log::*;
use serde::{Deserialize, Serialize};
use stable_hash::{FieldAddress, StableHash};
use tokio::{
    select,
    time::{Duration, Instant},
};
use tokio_mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    ReplyChannel,
};
use tokio_serde::{
    formats::{Bincode, SymmetricalBincode},
    Deserializer, Serializer,
};

use crate::network::connection_manager::ConnectionID;

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
            "<Node {}:{}-{}>",
            self.address, self.port, self.generation
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

#[derive(Clone, Debug)]
pub struct PeerConnection {
    connection_id: ConnectionID,
    // Nodes connect to seeds at startup, and the disconnect when they have enough
    // info about the network. If a peer is marked `is_temporary`, it won't count
    // towards the total number of connected peers and is a candidate for being
    // replaced with a new peer.
    is_temporary: bool,
}

impl PeerConnection {
    fn new(connection_id: ConnectionID, is_temporary: bool) -> Self {
        Self {
            connection_id,
            is_temporary,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    address: NodeAddress,
    last_heartbeat: u32,
    last_heartbeat_timestamp: Option<Instant>,
    distance: u32,
    // The last seq number at which the distance was observed
    distance_seq: u32,
    peer_connection: Option<PeerConnection>,
}

impl Node {
    fn new(address: NodeAddress, distance: u32, peer_connection: Option<PeerConnection>) -> Self {
        Self {
            address,
            last_heartbeat: 0,
            last_heartbeat_timestamp: None,
            distance,
            distance_seq: 0,
            peer_connection,
        }
    }

    fn from_heartbeat(heartbeat: &Heartbeat) -> Self {
        Self {
            address: heartbeat.node_address.clone(),
            last_heartbeat: heartbeat.seq,
            last_heartbeat_timestamp: Some(Instant::now()),
            distance: heartbeat.distance,
            distance_seq: heartbeat.seq,
            peer_connection: None,
        }
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
    // TODO: Mark node as unhealthy when heartbeats don't arrive for some time
    // TODO: Remove nodes from network when heartbeats don't arrive for even longer
    // TODO: Handle cases where A considers C dead, but B doesn't
    Heartbeat(Heartbeat),

    /// Each node sends a Goodbye message when shutting down cleanly.
    /// This helps other nodes maintain an up-to-date state of the network.
    /// Nodes propagate Goodbye messages similarly to Hello messages.
    Goodbye(NodeAddress),
}

#[async_trait]
pub trait Gossip {
    async fn receive_message(&self, connection_id: ConnectionID, bytes: Bytes);
    async fn get_nodes(&self) -> Result<Vec<(NodeHash, NodeAddress)>>;
    async fn stop(&self) -> Result<()>;
}

pub struct GossipConfig {
    pub heartbeat_interval: Duration,
    pub assume_dead_after_missed_heartbeats: u32,
    pub max_peers: u32,
    pub network_initialization_time: Duration,
}

enum GossipControlMessage {
    ReceiveMessage(ConnectionID, Bytes),
    GetPeers(ReplyChannel<Vec<(NodeHash, NodeAddress)>>),
    Stop(ReplyChannel<()>),
}

pub type NodeDiedCleanly = bool;

pub enum GossipNotification {
    // Node-related notifications
    NodeDiscovered(NodeAddress),
    NodeDied(NodeAddress, NodeDiedCleanly),

    // Requests
    SendMessage(ConnectionID, Bytes),
}

type NotificationChannel = tokio_mailbox_processor::NotificationChannel<GossipNotification>;
pub type KnownNodes = Vec<(NodeAddress, ConnectionID)>;

struct GossipImpl {
    mailbox: PlainMailboxProcessor<GossipControlMessage>,
}

#[async_trait]
impl Gossip for GossipImpl {
    async fn receive_message(&self, connection_id: ConnectionID, bytes: Bytes) {
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
        //TODO: return type
        self.mailbox
            .post_and_reply(GossipControlMessage::Stop)
            .await
            .map_err(Into::into)
    }
}

type Codec = SymmetricalBincode<GossipProtocolMessage>;

pub async fn start(
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
    nodes: HashMap<NodeHash, Node>,
    peers: HashSet<NodeHash>,
    my_heartbeat: u32,
    codec: Codec,
}

async fn body(
    mut message_receiver: MessageReceiver<GossipControlMessage>,
    my_address: NodeAddress,
    config: GossipConfig,
    known_nodes: KnownNodes,
    notification_channel: NotificationChannel,
) {
    let mut state = GossipState {
        config,
        my_address,
        notification_channel,
        my_heartbeat: 0,
        codec: Bincode::default(),
        // This is an index on nodes, and must be kept up-to-date.
        peers: known_nodes
            .iter()
            .map(|(node, _)| node.get_hash())
            .collect(),
        nodes: known_nodes
            .into_iter()
            .map(|(address, connection_id)| {
                (
                    address.get_hash(),
                    Node::new(address, 1, Some(PeerConnection::new(connection_id, true))),
                )
            })
            .collect(),
    };

    let mut heartbeat_timer = tokio::time::interval(state.config.heartbeat_interval);
    heartbeat_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // TODO: initialize peers after network initialization window has passed
    // TODO: sanitize and update peers every once in a while
    // TODO: remove nodes that missed heartbeats
    'main_loop: loop {
        select! {
            // This also handles immediately sending heartbeats to known nodes,
            // since the timer ticks once immediately
            _ = heartbeat_timer.tick() => {
                if let Err(f) = send_heartbeat(&mut state) {
                    error!("Failed to send heartbeat: {f}");
                }
            }

            msg = message_receiver.receive() => {
                match msg {
                    None => {
                        info!("All senders dropped, stopping gossip");
                        break 'main_loop;
                    }

                    Some(GossipControlMessage::ReceiveMessage(id, bytes)) => {
                        if let Err(f) = receive_message(
                            id,
                            bytes,
                            &mut state
                        ) {
                            warn!("Failed to receive message: {f}");
                        }
                    }

                    Some(GossipControlMessage::GetPeers(r)) => r.reply(
                        state.nodes
                            .iter()
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

fn get_peer_nodes(state: &GossipState) -> impl Iterator<Item = (&Node, &PeerConnection)> {
    state.peers.iter().map(|hash| {
        let node = state
            .nodes
            .get(hash)
            .with_context(|| format!("No node corresponding to peer {hash}"))
            .unwrap();
        let connection = node
            .peer_connection
            .as_ref()
            .with_context(|| format!("Node {hash} was in peers has no peer connection"))
            .unwrap();
        (node, connection)
    })
}

fn send_heartbeat(state: &mut GossipState) -> Result<()> {
    state.my_heartbeat += 1;

    debug!("Sending heartbeat #{}", state.my_heartbeat);

    let message = GossipProtocolMessage::Heartbeat(Heartbeat {
        node_address: state.my_address.clone(),
        seq: state.my_heartbeat,
        distance: 1,
    });

    send_protocol_message(message, state)
}

fn send_goodbye(state: &mut GossipState) -> Result<()> {
    debug!("Sending goodbye");

    let message = GossipProtocolMessage::Goodbye(state.my_address.clone());

    send_protocol_message(message, state)
}

fn send_protocol_message(message: GossipProtocolMessage, state: &mut GossipState) -> Result<()> {
    debug!("Sending protocol message {message:?}");

    let message_bytes = Pin::new(&mut state.codec)
        .serialize(&message)
        .context("Failed to serialize goodbye message")?;

    for (_, peer) in get_peer_nodes(&state) {
        state
            .notification_channel
            .send(GossipNotification::SendMessage(
                peer.connection_id,
                message_bytes.clone(),
            ));
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
    match message {
        GossipProtocolMessage::Heartbeat(heartbeat) => {
            let hash = heartbeat.node_address.get_hash();
            match state.nodes.entry(hash) {
                Entry::Occupied(mut entry) => {
                    debug!(
                        "Heartbeat #{} from known node {}",
                        heartbeat.seq, heartbeat.node_address
                    );

                    let mut node = entry.get_mut();

                    if node.last_heartbeat < heartbeat.seq {
                        node.last_heartbeat = heartbeat.seq;
                        node.last_heartbeat_timestamp = Some(Instant::now());
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
        }

        GossipProtocolMessage::Goodbye(node) => {
            let hash = node.get_hash();
            match state.nodes.remove(&hash) {
                Some(node) => {
                    state.peers.remove(&hash);
                    debug!("Goodbye from node {}", node.address);
                    state
                        .notification_channel
                        .send(GossipNotification::NodeDied(node.address, true));
                }

                None => {
                    debug!("Goodbye from unknown node {node}, ignoring");
                }
            }
        }
    }

    Ok(())
}
