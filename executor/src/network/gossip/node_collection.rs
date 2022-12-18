use std::{
    collections::{
        hash_map::{self, Entry},
        HashMap, HashSet,
    },
    net::IpAddr,
};

use anyhow::{bail, Context, Result};
use mu_stack::StackID;
use tokio::time::Instant;

use super::super::ConnectionID;

use super::{Heartbeat, NodeAddress, NodeHash};

// use crate::network::gossip::*;

pub struct NodeCollection {
    nodes: HashMap<NodeHash, Node>,
    peers: HashSet<NodeHash>,
    nodes_by_addr_and_port: HashMap<(IpAddr, u16), NodeHash>,
}

impl NodeCollection {
    pub(super) fn new(known_nodes: KnownNodes) -> Self {
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
                    let hash = address.get_hash();
                    (
                        hash,
                        Node::Peer(Peer::Temporary(TemporaryPeer(
                            hash,
                            NodeInfo::new(address, 1, now),
                            connection_id,
                        ))),
                    )
                })
                .collect(),
        }
    }

    pub(super) fn get_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub(super) fn get_node(&self, hash: &NodeHash) -> Option<&Node> {
        self.nodes.get(hash)
    }

    // TODO: unsafe, might update node hash
    pub(super) fn get_node_mut(&mut self, hash: &NodeHash) -> Option<&mut Node> {
        self.nodes.get_mut(hash)
    }

    pub(super) fn get_nodes_and_hashes(&self) -> impl Iterator<Item = (&NodeHash, &Node)> {
        self.nodes.iter()
    }

    pub(super) fn get_peers_and_hashes(&self) -> impl Iterator<Item = (&NodeHash, &Peer)> {
        self.peers.iter().map(|hash| {
            let node = self
                .nodes
                .get(hash)
                .with_context(|| format!("No node corresponding to peer {hash}"))
                .unwrap();
            match node {
                Node::Peer(peer) => (hash, peer),
                Node::RemoteNode(_) => panic!("Node {hash} was in peers is not a peer"),
            }
        })
    }

    pub(super) fn get_peers(&self) -> impl Iterator<Item = &Peer> {
        self.get_peers_and_hashes().map(|(_, peer)| peer)
    }

    pub(super) fn get_temporary_peers_and_hashes(
        &self,
    ) -> impl Iterator<Item = (&NodeHash, &TemporaryPeer)> {
        self.get_peers_and_hashes().filter_map(|(h, p)| match p {
            Peer::Temporary(p) => Some((h, p)),
            Peer::Permanent(_) => None,
        })
    }

    pub(super) fn get_permanent_peers_and_hashes(
        &self,
    ) -> impl Iterator<Item = (&NodeHash, &PermanentPeer)> {
        self.get_peers_and_hashes().filter_map(|(h, p)| match p {
            Peer::Temporary(_) => None,
            Peer::Permanent(p) => Some((h, p)),
        })
    }

    pub(super) fn hash_entry<'a>(&'a mut self, hash: &NodeHash) -> HashEntry<'a> {
        match self.nodes.entry(*hash) {
            Entry::Occupied(occ) => HashEntry::OccupiedBySameGeneration(OccupiedBySameGeneration {
                inner: occ,
                peers: &mut self.peers,
            }),
            Entry::Vacant(vac) => HashEntry::Vacant(Vacant {
                peers: &mut self.peers,
                nodes_by_addr_and_port: &mut self.nodes_by_addr_and_port,
                inner: vac,
            }),
        }
    }

    pub(super) fn node_entry<'a>(&'a mut self, node_address: &NodeAddress) -> NodeEntry<'a> {
        let input_hash = node_address.get_hash();
        match self
            .nodes_by_addr_and_port
            .get(&(node_address.address, node_address.port))
        {
            None => match self.nodes.entry(input_hash) {
                hash_map::Entry::Vacant(vac) => NodeEntry::Vacant(Vacant {
                    peers: &mut self.peers,
                    nodes_by_addr_and_port: &mut self.nodes_by_addr_and_port,
                    inner: vac,
                }),
                hash_map::Entry::Occupied(_) => {
                    panic!("nodes is out of sync with nodes_by_addr_and_port")
                }
            },
            Some(old_hash) if *old_hash == input_hash => match self.nodes.entry(input_hash) {
                hash_map::Entry::Occupied(occ) => {
                    NodeEntry::Occupied(OccupiedByGeneration::Same(OccupiedBySameGeneration {
                        peers: &mut self.peers,
                        inner: occ,
                    }))
                }
                hash_map::Entry::Vacant(_) => {
                    panic!("nodes is out of sync with nodes_by_addr_and_port")
                }
            },
            Some(old_hash) => match self.nodes.get(old_hash) {
                Some(node) => {
                    let generation = node.info().address.generation;
                    if generation < node_address.generation {
                        NodeEntry::Occupied(OccupiedByGeneration::Older(
                            OccupiedByOlderGeneration {
                                old_hash: *old_hash,
                                nodes: &mut self.nodes,
                                nodes_by_addr_and_port: &mut self.nodes_by_addr_and_port,
                                peers: &mut self.peers,
                            },
                        ))
                    } else {
                        NodeEntry::Occupied(OccupiedByGeneration::Newer())
                    }
                }
                None => {
                    panic!("nodes is out of sync with nodes_by_addr_and_port")
                }
            },
        }
    }

    pub(super) fn remove(&mut self, hash: &NodeHash) -> Option<Node> {
        match self.nodes.remove(hash) {
            Some(node) => {
                self.peers.remove(hash);
                let address = &node.info().address;
                self.nodes_by_addr_and_port
                    .remove(&(address.address, address.port));
                Some(node)
            }
            None => None,
        }
    }
}

pub type KnownNodes = Vec<(NodeAddress, ConnectionID)>;

#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub(super) address: NodeAddress,
    pub(super) last_heartbeat: u32,
    pub(super) last_heartbeat_timestamp: Instant,
    pub(super) distance: u32,
    // The last seq number at which the distance was observed
    pub(super) distance_seq: u32,
    pub(super) deployed_stacks: HashSet<StackID>,
}

impl NodeInfo {
    fn new(address: NodeAddress, distance: u32, last_heartbeat: Instant) -> Self {
        Self {
            address,
            last_heartbeat: 0,
            last_heartbeat_timestamp: last_heartbeat,
            distance,
            distance_seq: 0,
            deployed_stacks: HashSet::new(),
        }
    }

    pub(super) fn from_heartbeat(heartbeat: &Heartbeat) -> Self {
        Self {
            address: heartbeat.node_address.clone(),
            last_heartbeat: heartbeat.seq,
            last_heartbeat_timestamp: Instant::now(),
            distance: heartbeat.distance,
            distance_seq: heartbeat.seq,
            deployed_stacks: heartbeat.deployed_stacks.iter().cloned().collect(),
        }
    }

    pub fn get_hash(&self) -> NodeHash {
        self.address.get_hash()
    }
}

#[derive(Debug)]
pub(super) enum Node {
    RemoteNode(RemoteNode),
    Peer(Peer),
}

impl Node {
    pub fn info(&self) -> &NodeInfo {
        match self {
            Node::RemoteNode(n) => &n.1,
            Node::Peer(p) => p.info(),
        }
    }

    // TODO: unsafe: if address is changed, node hash will also change
    pub fn info_mut(&mut self) -> &mut NodeInfo {
        match self {
            Node::RemoteNode(n) => &mut n.1,
            Node::Peer(p) => p.info_mut(),
        }
    }

    pub fn into_info(self) -> NodeInfo {
        match self {
            Node::RemoteNode(n) => n.1,
            Node::Peer(p) => p.into_info(),
        }
    }
}

#[derive(Debug)]
pub(super) enum Peer {
    // Nodes connect to seeds at startup, and the disconnect when they have enough
    // info about the network. If a peer is marked `is_temporary`, it won't count
    // towards the total number of connected peers and is a candidate for being
    // replaced with a new peer.
    Temporary(TemporaryPeer),
    Permanent(PermanentPeer),
}

impl Peer {
    pub fn hash(&self) -> &NodeHash {
        match self {
            Peer::Temporary(t) => &t.0,
            Peer::Permanent(p) => &p.0,
        }
    }

    pub fn info(&self) -> &NodeInfo {
        match self {
            Peer::Temporary(t) => &t.1,
            Peer::Permanent(p) => &p.1,
        }
    }

    pub fn info_mut(&mut self) -> &mut NodeInfo {
        match self {
            Peer::Temporary(t) => &mut t.1,
            Peer::Permanent(p) => &mut p.1,
        }
    }

    pub fn into_info(self) -> NodeInfo {
        match self {
            Peer::Temporary(t) => t.1,
            Peer::Permanent(p) => p.1,
        }
    }

    pub fn connection_id(&self) -> ConnectionID {
        match self {
            Peer::Temporary(t) => t.2,
            Peer::Permanent(p) => p.2,
        }
    }

    pub fn set_connection_id(&mut self, connection_id: ConnectionID) {
        match self {
            Peer::Temporary(t) => t.2 = connection_id,
            Peer::Permanent(p) => p.2 = connection_id,
        }
    }
}

#[derive(Debug)]
pub(super) struct RemoteNode(NodeHash, NodeInfo);

impl RemoteNode {
    pub fn info(&self) -> &NodeInfo {
        &self.1
    }
}

#[derive(Debug)]
pub(super) struct TemporaryPeer(NodeHash, NodeInfo, ConnectionID);

#[derive(Debug)]
pub(super) struct PermanentPeer(NodeHash, NodeInfo, ConnectionID);

pub(super) enum NodeEntry<'a> {
    Occupied(OccupiedByGeneration<'a>),
    Vacant(Vacant<'a>),
}

pub(super) enum HashEntry<'a> {
    OccupiedBySameGeneration(OccupiedBySameGeneration<'a>),
    Vacant(Vacant<'a>),
}

pub(super) enum OccupiedByGeneration<'a> {
    Same(OccupiedBySameGeneration<'a>),
    Older(OccupiedByOlderGeneration<'a>),
    Newer(),
}

pub(super) struct OccupiedBySameGeneration<'a> {
    inner: hash_map::OccupiedEntry<'a, NodeHash, Node>,
    peers: &'a mut HashSet<NodeHash>,
}

impl<'a> OccupiedBySameGeneration<'a> {
    pub fn get(&self) -> &Node {
        self.inner.get()
    }

    pub fn get_mut(&mut self) -> &mut Node {
        self.inner.get_mut()
    }

    // TODO: the current structure doesn't allow it, but we *should* be able to
    // change this function's signature so it doesn't fail
    pub fn promote_to_peer(&mut self, connection_id: ConnectionID) -> Result<()> {
        match self.inner.get() {
            Node::RemoteNode(remote) => {
                let hash = remote.0;
                self.inner.insert(Node::Peer(Peer::Permanent(PermanentPeer(
                    hash,
                    remote.1.clone(), // TODO: can't we move this?
                    connection_id,
                ))));
                self.peers.insert(hash);
                Ok(())
            }
            Node::Peer(p) => bail!("Node {} was not a remote", p.hash()),
        }
    }

    pub fn promote_to_permanent(&mut self) -> Result<()> {
        match self.inner.get() {
            Node::Peer(Peer::Temporary(temp)) => {
                self.inner.insert(Node::Peer(Peer::Permanent(PermanentPeer(
                    temp.0,
                    temp.1.clone(), // TODO: can't we move this?
                    temp.2,
                ))));
                Ok(())
            }
            Node::RemoteNode(n) => bail!("Node {} was not a remote", n.0),
            Node::Peer(Peer::Permanent(p)) => bail!("Node {} was not a remote", p.0),
        }
    }
}

pub(super) struct OccupiedByOlderGeneration<'a> {
    // Since we also need to modify the map itself, we can't store an Entry here
    old_hash: NodeHash,
    nodes: &'a mut HashMap<NodeHash, Node>,
    nodes_by_addr_and_port: &'a mut HashMap<(IpAddr, u16), NodeHash>,
    peers: &'a mut HashSet<NodeHash>,
}

impl<'a> OccupiedByOlderGeneration<'a> {
    pub fn get(&self) -> &Node {
        // We know the node is in the map, we just can't keep the Entry around
        self.nodes.get(&self.old_hash).unwrap()
    }

    pub fn update_generation(self, generation: u128) -> OccupiedBySameGeneration<'a> {
        let address = &self.get().info().address;

        match self
            .nodes_by_addr_and_port
            .entry((address.address, address.port))
        {
            Entry::Vacant(_) => panic!("No previous addr_and_port entry for this node"),

            Entry::Occupied(mut addr_and_port) => {
                let mut node = self.nodes.remove(&self.old_hash).unwrap();
                self.peers.remove(&self.old_hash);

                let info = node.info_mut();
                info.address.generation = generation;
                // A new generation will almost certainly start sending lower heartbeats,
                // since the process was restarted and had to start back at zero
                info.last_heartbeat = 0;
                info.last_heartbeat_timestamp = Instant::now();
                let new_hash = info.get_hash();

                addr_and_port.insert(new_hash);
                self.peers.insert(new_hash);

                self.nodes.insert(new_hash, node);

                let entry = match self.nodes.entry(new_hash) {
                    Entry::Occupied(occ) => occ,
                    Entry::Vacant(_) => panic!("Impossible, we just inserted the node"),
                };
                OccupiedBySameGeneration {
                    inner: entry,
                    peers: self.peers,
                }
            }
        }
    }
}

pub(super) struct Vacant<'a> {
    peers: &'a mut HashSet<NodeHash>,
    nodes_by_addr_and_port: &'a mut HashMap<(IpAddr, u16), NodeHash>,
    inner: hash_map::VacantEntry<'a, NodeHash, Node>,
}

impl<'a> Vacant<'a> {
    pub fn insert_remote(self, info: NodeInfo) -> &'a mut Node {
        let hash = *self.inner.key();
        assert_eq!(
            hash,
            info.get_hash(),
            "Cannot insert node with different hash in vacant slot"
        );

        self.nodes_by_addr_and_port
            .insert((info.address.address, info.address.port), hash);

        self.inner.insert(Node::RemoteNode(RemoteNode(hash, info)))
    }

    #[allow(dead_code)]
    pub fn insert_peer(self, info: NodeInfo, connection_id: ConnectionID) -> &'a mut Node {
        let hash = *self.inner.key();
        assert_eq!(
            hash,
            info.get_hash(),
            "Cannot insert node with different hash in vacant slot"
        );

        self.nodes_by_addr_and_port
            .insert((info.address.address, info.address.port), hash);

        self.peers.insert(hash);

        self.inner.insert(Node::Peer(Peer::Permanent(PermanentPeer(
            hash,
            info,
            connection_id,
        ))))
    }
}
