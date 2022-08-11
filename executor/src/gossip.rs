use std::{collections::HashMap, net::IpAddr, pin::Pin, time::SystemTime};

use anyhow::{Context, Ok, Result};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use log::*;
use serde::{Deserialize, Serialize};
use stable_hash::{FieldAddress, StableHash};
use tokio::{select, time::Duration};
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
#[derive(Serialize, Deserialize, Clone)]
pub struct Node {
    pub address: IpAddr,
    pub port: u16,
    pub generation: u128,
}

pub enum NodeStatus {
    Healthy,
    Unhealthy,
    Disconnected,
}

impl StableHash for Node {
    fn stable_hash<H: stable_hash::StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        self.address
            .to_string()
            .stable_hash(field_address.child(0), state);
        self.port.stable_hash(field_address.child(1), state);
        self.generation.stable_hash(field_address.child(2), state);
    }
}

impl Node {
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

#[derive(Clone)]
pub struct Peer {
    last_heartbeat: u32,
    node: Node,
    connection_id: ConnectionID,
}

impl Peer {
    fn new(node: Node, connection_id: ConnectionID) -> Self {
        Self {
            last_heartbeat: 0,
            node,
            connection_id,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Heartbeat {
    node: Node,
    seq: u32,
}

// TODO: replace with version-tolerant solution (gRPC?)
#[derive(Serialize, Deserialize, Clone)]
enum GossipProtocolMessage {
    /// Each node sends out heartbeat messages to all peers at regular intervals.
    // TODO: Mark node as unhealthy when heartbeats don't arrive for some time
    // TODO: Remove nodes from network when heartbeats don't arrive for even longer
    // TODO: Handle cases where A considers C dead, but B doesn't
    Heartbeat(Heartbeat),

    /// Each node sends a Goodbye message when shutting down cleanly.
    /// This helps other nodes maintain an up-to-date state of the network.
    /// Nodes propagate Goodbye messages similarly to Hello messages.
    Goodbye(Node),
}

#[async_trait]
pub trait Gossip {
    async fn receive_message(&self, bytes: Bytes);
    async fn get_peers(&self) -> Result<Vec<(NodeHash, Peer)>>;
    async fn stop(self) -> Result<()>;
}

pub struct GossipConfig {
    pub heartbeat_interval: Duration,
}

enum GossipControlMessage {
    ReceiveMessage(Bytes),
    GetPeers(ReplyChannel<Vec<(NodeHash, Peer)>>),
    Stop(ReplyChannel<()>),
}

pub enum GossipNotification {
    // Peer-related notifications
    NewPeerAvailable(Peer),
    PeerStatusUpdated(Peer, NodeStatus),
    PeerLeft(Peer),

    // Requests
    SendMessage(ConnectionID, Bytes),
}

type NotificationChannel = tokio_mailbox_processor::NotificationChannel<GossipNotification>;
type KnownNodes = Vec<(Node, ConnectionID)>;

pub struct GossipImpl {
    mailbox: PlainMailboxProcessor<GossipControlMessage>,
}

#[async_trait]
impl Gossip for GossipImpl {
    async fn receive_message(&self, bytes: Bytes) {
        self.mailbox
            .post_and_forget(GossipControlMessage::ReceiveMessage(bytes));
    }

    async fn get_peers(&self) -> Result<Vec<(NodeHash, Peer)>> {
        self.mailbox
            .post_and_reply(GossipControlMessage::GetPeers)
            .await
            .map_err(Into::into)
    }

    async fn stop(self) -> Result<()> {
        //TODO: return type
        self.mailbox
            .post_and_reply(GossipControlMessage::Stop)
            .await
            .map_err(Into::into)
    }
}

type PinnedCodec<'a> = Pin<&'a mut SymmetricalBincode<GossipProtocolMessage>>;

pub async fn start(
    my_node: Node,
    config: GossipConfig,
    known_nodes: KnownNodes,
    notification_channel: NotificationChannel,
) -> Result<GossipImpl> {
    let mailbox = PlainMailboxProcessor::start(
        move |_mb, r| body(r, my_node, config, known_nodes, notification_channel),
        10000,
    );

    Ok(GossipImpl { mailbox })
}

async fn body(
    mut message_receiver: MessageReceiver<GossipControlMessage>,
    my_node: Node,
    config: GossipConfig,
    known_nodes: KnownNodes,
    notification_channel: NotificationChannel,
) {
    let mut peers = known_nodes
        .into_iter()
        .map(|(node, id)| (stable_hash::fast_stable_hash(&node), Peer::new(node, id)))
        .collect::<HashMap<NodeHash, Peer>>();

    let mut my_heartbeat: u32 = 0;

    let mut codec = Bincode::default();

    let mut timer = tokio::time::interval(config.heartbeat_interval);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    'main_loop: loop {
        select! {
            _ = timer.tick() => {
                if let Err(f) = send_heartbeat(
                    &mut my_heartbeat,
                    &my_node,
                    &peers,
                    &notification_channel,
                    Pin::new(&mut codec),
                ) {
                    error!("Failed to send heartbeat: {f}");
                }
            }

            msg = message_receiver.receive() => {
                match msg {
                    None => {
                        info!("All senders dropped, stopping gossip");
                        break 'main_loop;
                    }

                    Some(GossipControlMessage::ReceiveMessage(bytes)) => {
                        if let Err(f) = receive_message(bytes, Pin::new(&mut codec), &mut peers, &notification_channel) {
                            warn!("Failed to receive heartbeat: {f}");
                        }
                    }

                    Some(GossipControlMessage::GetPeers(r)) => r.reply(peers.iter().map(|(k, v)| (*k, v.clone())).collect()),

                    Some(GossipControlMessage::Stop(r)) => {
                        r.reply(());
                        break 'main_loop;
                    }
                }
            }
        }
    }
}

fn send_heartbeat<'a>(
    my_heartbeat: &mut u32,
    my_node: &Node,
    peers: &HashMap<NodeHash, Peer>,
    notification_channel: &NotificationChannel,
    codec: PinnedCodec<'a>,
) -> Result<()> {
    *my_heartbeat += 1;

    debug!("Sending heartbeat #{}", *my_heartbeat);

    let message = GossipProtocolMessage::Heartbeat(Heartbeat {
        node: my_node.clone(),
        seq: *my_heartbeat,
    });

    let message_bytes = codec
        .serialize(&message)
        .context("Failed to serialize heartbeat message")?;

    for peer in peers.values() {
        notification_channel.send(GossipNotification::SendMessage(
            peer.connection_id,
            message_bytes.clone(),
        ));
    }

    Ok(())
}

fn receive_message<'a>(
    bytes: Bytes,
    codec: PinnedCodec<'a>,
    peers: &mut HashMap<NodeHash, Peer>,
    notification_channel: &NotificationChannel,
) -> Result<()> {
    // TODO: why does deserialize take a BytesMut? Is there a way to deserialize from Bytes directly?
    let buf: &[u8] = &bytes;
    let bytes_mut: BytesMut = buf.into();
    let message = codec
        .deserialize(&bytes_mut)
        .context("Failed to deserialize message")?;
    match message {
        GossipProtocolMessage::Heartbeat(heartbeat) => todo!(),
        GossipProtocolMessage::Goodbye(node) => todo!(),
    }
    Ok(())
}
