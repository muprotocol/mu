use std::collections::hash_map;

use crate::network::gossip::*;

pub struct NodeCollection {
    nodes: HashMap<NodeHash, Node>,
    peers: HashSet<NodeHash>,
    nodes_by_addr_and_port: HashMap<(IpAddr, u16), NodeHash>,
}

impl NodeCollection {
    pub fn new(known_nodes: KnownNodes) -> Self {
        let now = Instant::now();
        Self {
            peers: known_nodes
                .iter()
                .map(|(node, _)| node.get_hash())
                .collect(),
            nodes_by_addr_and_port: known_nodes
                .iter()
                .map(|(node, _)| ((node.address, node.port), node.get_hash()))
                .collect(),
            nodes: known_nodes
                .into_iter()
                .map(|(address, connection_id)| {
                    (
                        address.get_hash(),
                        Node::new(
                            address,
                            1,
                            Some(PeerConnection::new(connection_id, true)),
                            now,
                        ),
                    )
                })
                .collect(),
        }
    }

    pub fn get_nodes<'a>(&'a self) -> impl Iterator<Item = NodeStatus<'a>> {
        self.nodes
            .iter()
            .map(|(hash, node)| Self::get_node_status(hash, node))
    }

    pub fn get_node<'a>(&'a self, hash: NodeHash) -> Option<NodeStatus<'a>> {
        self.nodes
            .get(&hash)
            .map(|node| Self::get_node_status(&hash, node))
    }

    fn get_node_status<'a>(hash: &'a NodeHash, node: &'a Node) -> NodeStatus {
        match &node.peer_connection {
            None => NodeStatus::RemoteNode(RemoteNode(*hash, node)),
            Some(peer_connection) if peer_connection.is_temporary => {
                NodeStatus::Peer(PeerStatus::TemporaryPeer(TemporaryPeer(
                    *hash,
                    node,
                    peer_connection.connection_id,
                )))
            }
            Some(peer_connection) => NodeStatus::Peer(PeerStatus::PermanentPeer(PermanentPeer(
                *hash,
                node,
                peer_connection.connection_id,
            ))),
        }
    }

    pub fn get_nodes_and_hashes(&self) -> impl Iterator<Item = (&NodeHash, &Node)> {
        self.nodes.iter()
    }

    pub fn get_peers_raw(&self) -> impl Iterator<Item = (&NodeHash, &Node, &PeerConnection)> {
        self.peers.iter().map(|hash| {
            let node = self
                .nodes
                .get(hash)
                .with_context(|| format!("No node corresponding to peer {hash}"))
                .unwrap();
            let connection = node
                .peer_connection
                .as_ref()
                .with_context(|| format!("Node {hash} was in peers has no peer connection"))
                .unwrap();
            (hash, node, connection)
        })
    }

    pub fn get_peers<'a>(&'a self) -> impl Iterator<Item = PeerStatus<'a>> {
        self.get_peers_raw().map(|(hash, node, peer)| {
            if peer.is_temporary {
                PeerStatus::TemporaryPeer(TemporaryPeer(*hash, node, peer.connection_id))
            } else {
                PeerStatus::PermanentPeer(PermanentPeer(*hash, node, peer.connection_id))
            }
        })
    }

    pub fn get_node_entry<'a>(&'a self, node_address: NodeAddress) -> Entry<'a> {
        let input_hash = node_address.get_hash();
        match self
            .nodes_by_addr_and_port
            .get(&(node_address.address, node_address.port))
        {
            None => match self.nodes.entry(input_hash) {
                hash_map::Entry::Vacant(v) => Entry::Vacant(Vacant {
                    col: self,
                    inner: v,
                }),
                hash_map::Entry::Occupied(_) => {
                    panic!("nodes is out of sync with nodes_by_addr_and_port")
                }
            },
            Some(old_hash) if *old_hash == input_hash => match self.nodes.entry(input_hash) {
                hash_map::Entry::Occupied(occ) => Entry::Occupied(Occupied {
                    inner: occ,
                    col: self,
                }),
                hash_map::Entry::Vacant(_) => {
                    panic!("nodes is out of sync with nodes_by_addr_and_port")
                }
            },
            Some(old_hash) => match self.nodes.entry(*old_hash) {
                hash_map::Entry::Occupied(occ) => {
                    let generation = occ.get().address.generation;
                    if generation < node_address.generation {
                        Entry::OccupiedByOlderGeneration(OccupiedByOlderGeneration {
                            inner: occ,
                            col: self,
                        })
                    } else {
                        Entry::OccupiedByNewerGeneration(OccupiedByNewerGeneration {
                            inner: occ,
                            col: self,
                        })
                    }
                }
                hash_map::Entry::Vacant(_) => {
                    panic!("nodes is out of sync with nodes_by_addr_and_port")
                }
            },
        }
    }
}

pub type KnownNodes = Vec<(NodeAddress, ConnectionID)>;

#[derive(Clone, Debug)]
struct PeerConnection {
    pub(super) connection_id: ConnectionID,
    // Nodes connect to seeds at startup, and the disconnect when they have enough
    // info about the network. If a peer is marked `is_temporary`, it won't count
    // towards the total number of connected peers and is a candidate for being
    // replaced with a new peer.
    pub(super) is_temporary: bool,
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
    pub(super) address: NodeAddress,
    pub(super) last_heartbeat: u32,
    pub(super) last_heartbeat_timestamp: Instant,
    pub(super) distance: u32,
    // The last seq number at which the distance was observed
    pub(super) distance_seq: u32,
    pub(super) peer_connection: Option<PeerConnection>,
}

impl Node {
    fn new(
        address: NodeAddress,
        distance: u32,
        peer_connection: Option<PeerConnection>,
        last_heartbeat: Instant,
    ) -> Self {
        Self {
            address,
            last_heartbeat: 0,
            last_heartbeat_timestamp: last_heartbeat,
            distance,
            distance_seq: 0,
            peer_connection,
        }
    }

    fn from_heartbeat(heartbeat: &Heartbeat) -> Self {
        Self {
            address: heartbeat.node_address.clone(),
            last_heartbeat: heartbeat.seq,
            last_heartbeat_timestamp: Instant::now(),
            distance: heartbeat.distance,
            distance_seq: heartbeat.seq,
            peer_connection: None,
        }
    }
}

pub(super) enum NodeStatus<'a> {
    RemoteNode(RemoteNode<'a>),
    Peer(PeerStatus<'a>),
}

pub(super) enum PeerStatus<'a> {
    TemporaryPeer(TemporaryPeer<'a>),
    PermanentPeer(PermanentPeer<'a>),
}

pub(super) struct RemoteNode<'a>(NodeHash, &'a Node);

pub(super) struct TemporaryPeer<'a>(NodeHash, &'a Node, ConnectionID);

pub(super) struct PermanentPeer<'a>(NodeHash, &'a Node, ConnectionID);

pub(super) enum Entry<'a> {
    Occupied(Occupied<'a>),
    OccupiedByOlderGeneration(OccupiedByOlderGeneration<'a>),
    OccupiedByNewerGeneration(OccupiedByNewerGeneration<'a>),
    Vacant(Vacant<'a>),
}

pub(super) struct Occupied<'a> {
    col: &'a NodeCollection,
    inner: hash_map::OccupiedEntry<'a, NodeHash, Node>,
}

pub(super) struct OccupiedByOlderGeneration<'a> {
    col: &'a NodeCollection,
    inner: hash_map::OccupiedEntry<'a, NodeHash, Node>,
}

pub(super) struct OccupiedByNewerGeneration<'a> {
    col: &'a NodeCollection,
    inner: hash_map::OccupiedEntry<'a, NodeHash, Node>,
}

pub(super) struct Vacant<'a> {
    col: &'a NodeCollection,
    inner: hash_map::VacantEntry<'a, NodeHash, Node>,
}
