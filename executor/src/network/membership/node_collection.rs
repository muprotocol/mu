use std::{collections::HashMap, net::IpAddr};

use crate::network::NodeHash;

use super::RemoteNodeInfo;

#[derive(Default)]
pub(super) struct NodeCollection {
    nodes: HashMap<NodeHash, RemoteNodeInfo>,
    nodes_by_addr_and_port: HashMap<(IpAddr, u16), NodeHash>,
}

impl NodeCollection {
    pub fn new(nodes: Vec<RemoteNodeInfo>) -> Self {
        let result = Self {
            nodes_by_addr_and_port: nodes
                .iter()
                .map(|n| ((n.address.address, n.address.port), n.address.get_hash()))
                .collect(),
            nodes: nodes
                .into_iter()
                .map(|n| (n.address.get_hash(), n))
                .collect(),
        };
        assert!(
            result.nodes.len() == result.nodes_by_addr_and_port.len(),
            "Duplicate node addresses found"
        );
        result
    }

    pub(super) fn get_nodes(&self) -> impl Iterator<Item = &RemoteNodeInfo> {
        self.nodes.values()
    }

    pub(super) fn get_node(&self, hash: &NodeHash) -> Option<&RemoteNodeInfo> {
        self.nodes.get(hash)
    }

    pub(super) fn get_by_address(&self, address: &(IpAddr, u16)) -> Option<&RemoteNodeInfo> {
        self.nodes_by_addr_and_port
            .get(address)
            .map(|hash| self.nodes.get(hash).expect("Index out of sync"))
    }

    pub(super) fn insert(&mut self, node: RemoteNodeInfo) -> bool {
        let hash = node.address.get_hash();
        if self
            .nodes_by_addr_and_port
            .get(&(node.address.address, node.address.port))
            .is_some()
        {
            // No need to check self.nodes, this check covers it
            return false;
        }

        self.nodes_by_addr_and_port
            .insert((node.address.address, node.address.port), hash);
        self.nodes.insert(hash, node);
        true
    }

    #[allow(dead_code)]
    pub(super) fn update(
        &mut self,
        hash: &NodeHash,
        update: impl FnOnce(RemoteNodeInfo) -> RemoteNodeInfo,
    ) -> bool {
        match self.remove(hash) {
            None => false,
            Some(node) => {
                if !self.insert(update(node)) {
                    panic!("Update resulted in duplicate node address");
                }

                true
            }
        }
    }

    pub(super) fn update_in_place(
        &mut self,
        hash: &NodeHash,
        update: impl FnOnce(&mut RemoteNodeInfo),
    ) -> bool {
        match self.nodes.get_mut(hash) {
            None => false,
            Some(node) => {
                update(node);

                if node.address.get_hash() != *hash {
                    panic!("Update resulted in different node hash");
                }

                true
            }
        }
    }

    pub(super) fn remove(&mut self, hash: &NodeHash) -> Option<RemoteNodeInfo> {
        match self.nodes.remove(hash) {
            Some(node) => {
                let address = &node.address;
                self.nodes_by_addr_and_port
                    .remove(&(address.address, address.port));
                Some(node)
            }
            None => None,
        }
    }
}
