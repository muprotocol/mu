pub mod protobuf;
pub mod protos;

use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    str::FromStr,
};

#[rustfmt::skip]
use ::protobuf::Message;
use anyhow::{anyhow, bail, Result};
use base58::{FromBase58, ToBase58};
use borsh::{BorshDeserialize, BorshSerialize};
use bytes::{BufMut, Bytes};
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use thiserror::Error;

pub const STACK_ID_SIZE: usize = 32;

#[derive(Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackID {
    SolanaPublicKey([u8; STACK_ID_SIZE]),
}

impl StackID {
    pub fn get_bytes(&self) -> &[u8; STACK_ID_SIZE] {
        match self {
            Self::SolanaPublicKey(key) => key,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(STACK_ID_SIZE + 1);
        match self {
            Self::SolanaPublicKey(key) => {
                res.push(1u8);
                res.put_slice(key);
            }
        }
        res
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != STACK_ID_SIZE + 1 {
            bail!("Incorrect byte count");
        }

        match bytes[0] {
            // We already know we have exactly enough bytes, so it's safe to unwrap
            1u8 => Ok(Self::SolanaPublicKey(bytes[1..].try_into().unwrap())),

            x => bail!("Unknown StackID discriminator {x}"),
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

#[derive(Error, Debug)]
pub enum ParseStackIDError {
    #[error("Invalid format")]
    InvalidFormat,

    #[error("Unknown variant")]
    UnknownVariant,

    #[error("Failed to parse: {0}")]
    FailedToParse(anyhow::Error),
}

impl FromStr for StackID {
    type Err = ParseStackIDError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 3 || s.chars().nth(1) != Some('_') {
            return Err(ParseStackIDError::InvalidFormat);
        }

        let variant_code = s.chars().next();

        match variant_code {
            Some('s') => {
                let (_, code) = s.split_at(2);
                let bytes = code.from_base58().map_err(|_| {
                    ParseStackIDError::FailedToParse(anyhow!("Failed to parse base58 string"))
                })?;
                Ok(Self::SolanaPublicKey(bytes.as_slice().try_into().map_err(
                    |_| ParseStackIDError::FailedToParse(anyhow!("Solana pubkey length mismatch")),
                )?))
            }
            _ => Err(ParseStackIDError::UnknownVariant),
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

    pub fn try_deserialize_proto(bytes: impl AsRef<[u8]>) -> Result<Stack> {
        crate::protos::stack::Stack::parse_from_bytes(bytes.as_ref())?.try_into()
    }

    pub fn key_value_tables(&self) -> impl Iterator<Item = &KeyValueTable> {
        self.services.iter().filter_map(|s| match s {
            Service::KeyValueTable(db) => Some(db),
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
    KeyValueTable(KeyValueTable),
    Gateway(Gateway),
    Function(Function),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyValueTable {
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

impl Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HttpMethod::Get => "Get",
            HttpMethod::Head => "Head",
            HttpMethod::Post => "Post",
            HttpMethod::Put => "Put",
            HttpMethod::Patch => "Patch",
            HttpMethod::Delete => "Delete",
            HttpMethod::Options => "Options",
        };
        std::fmt::Display::fmt(s, f)
    }
}
