use std::{
    fmt::{Debug, Display},
    net::IpAddr,
    time::SystemTime,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use stable_hash::{FieldAddress, StableHash};

pub mod connection_manager;
pub mod gossip;
pub mod rpc_handler;

pub type ConnectionID = u32;

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
        NodeHash(stable_hash::crypto_stable_hash(self))
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeHash(pub [u8; 32]);

impl Display for NodeHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", base64::encode(self.0))
    }
}

impl Debug for NodeHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self, f)
    }
}

#[derive(Clone, Debug)]
pub enum NodeConnection {
    Established(ConnectionID),
    NotEstablished(NodeAddress),
}
