pub mod protobuf;
pub mod protos;

use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    str::FromStr,
};

use ::protobuf::Message;
use anyhow::Result;
use base58::{FromBase58, ToBase58};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackID {
    SolanaPublicKey([u8; 32]),
}

impl StackID {
    pub fn get_bytes(&self) -> &[u8; 32] {
        match self {
            Self::SolanaPublicKey(key) => key,
        }
    }
}

impl Debug for StackID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SolanaPublicKey(pk) => {
                write!(f, "<Solana public key (base58): {}>", pk.to_base58())
            }
        }
    }
}

impl Display for StackID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SolanaPublicKey(pk) => write!(f, "s_{}", pk.to_base58()),
        }
    }
}

impl FromStr for StackID {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 3 || s.chars().nth(1) != Some('_') {
            return Err(());
        }

        let variant_code = s.chars().nth(0);

        match variant_code {
            Some('s') => {
                let (_, code) = s.split_at(2);
                let bytes = code.from_base58().map_err(|_| ())?;
                Ok(Self::SolanaPublicKey(
                    bytes.as_slice().try_into().map_err(|_| ())?,
                ))
            }
            _ => Err(()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct MegaByte(pub u32);

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct KiloByte(pub u32);

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Stack {
    pub name: String,
    pub version: String,
    pub services: Vec<Service>,
}

impl Stack {
    pub fn serialize_to_proto(self) -> Result<Bytes> {
        let stack: crate::protos::stack::Stack = self.into();
        Ok(stack.write_to_bytes()?.into())
    }

    pub fn try_deserialize_proto<'a>(bytes: impl Into<&'a [u8]>) -> Result<Stack> {
        crate::protos::stack::Stack::parse_from_bytes(bytes.into())?.try_into()
    }

    pub fn databases(&self) -> impl Iterator<Item = &Database> {
        self.services.iter().filter_map(|s| match s {
            Service::Database(db) => Some(db),
            _ => None,
        })
    }

    pub fn functions(&self) -> impl Iterator<Item = &Function> {
        self.services.iter().filter_map(|s| match s {
            Service::Function(func) => Some(func),
            _ => None,
        })
    }

    pub fn gateways(&self) -> impl Iterator<Item = &Gateway> {
        self.services.iter().filter_map(|s| match s {
            Service::Gateway(gw) => Some(gw),
            _ => None,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Service {
    Database(Database),
    Gateway(Gateway),
    Function(Function),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Database {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Gateway {
    pub name: String,
    pub endpoints: HashMap<String, Vec<GatewayEndpoint>>,
}

impl Gateway {
    // Strip leading slashes from urls, since that's the format rocket provides
    pub fn clone_normalized(&self) -> Self {
        let mut ep = HashMap::new();
        for (url, endpoint) in &self.endpoints {
            if let Some(stripped) = url.strip_prefix('/') {
                ep.insert(stripped.to_string(), endpoint.clone());
            } else {
                ep.insert(url.clone(), endpoint.clone());
            }
        }

        Self {
            name: self.name.clone(),
            endpoints: ep,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GatewayEndpoint {
    pub method: HttpMethod,
    pub route_to: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Function {
    pub name: String,
    pub binary: String,
    pub runtime: FunctionRuntime,
    pub env: HashMap<String, String>,
    pub memory_limit: MegaByte,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum FunctionRuntime {
    #[serde(rename = "wasi1.0")]
    Wasi1_0,
}
