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
use borsh::{BorshDeserialize, BorshSerialize};
use bytes::Bytes;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};

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

        let variant_code = s.chars().next();

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

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct AssemblyID {
    pub stack_id: StackID,
    pub assembly_name: String,
}

impl Display for AssemblyID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.stack_id, self.assembly_name)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FunctionID {
    pub assembly_id: AssemblyID,
    pub function_name: String,
}

impl FunctionID {
    pub fn stack_id(&self) -> &StackID {
        &self.assembly_id.stack_id
    }
}

impl Display for FunctionID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.assembly_id, self.function_name)
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
    pub route_to: AssemblyAndFunction,
}

#[derive(Debug, Clone)]
pub struct AssemblyAndFunction {
    pub assembly: String,
    pub function: String,
}

impl Serialize for AssemblyAndFunction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(format!("{}.{}", self.assembly, self.function).as_str())
    }
}

impl<'de> Deserialize<'de> for AssemblyAndFunction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(AssemblyAndFunctionDeserializeVisitor)
    }
}

struct AssemblyAndFunctionDeserializeVisitor;

impl<'de> Visitor<'de> for AssemblyAndFunctionDeserializeVisitor {
    type Value = AssemblyAndFunction;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "Two identifiers separated by a dot, such as `assembly_name.function_name`"
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let Some((asm, fun)) = v.split_once('.') else {
            return Err(E::invalid_value(serde::de::Unexpected::Str(v), &self));
        };
        Ok(AssemblyAndFunction {
            assembly: asm.to_string(),
            function: fun.to_string(),
        })
    }
}

#[derive(
    Serialize, Deserialize, BorshSerialize, BorshDeserialize, Debug, Clone, Copy, PartialEq, Eq,
)]
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
    pub runtime: AssemblyRuntime,
    pub env: HashMap<String, String>,
    pub memory_limit: byte_unit::Byte,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum AssemblyRuntime {
    #[serde(rename = "wasi1.0")]
    Wasi1_0,
}
