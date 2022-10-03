pub mod protobuf;
pub mod protos;

use std::{collections::HashMap, fmt::Display};

use ::protobuf::Message;
use anyhow::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackID(pub Uuid);

impl Display for StackID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

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

    pub fn try_deserialize_proto(bytes: Bytes) -> Result<Stack> {
        crate::protos::stack::Stack::parse_from_bytes(bytes.as_ref())?.try_into()
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
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum FunctionRuntime {
    #[serde(rename = "wasi1.0")]
    Wasi1_0,
}
