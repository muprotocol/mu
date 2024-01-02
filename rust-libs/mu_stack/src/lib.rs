pub mod protobuf;
pub mod protos;
pub mod string_serialization;
mod validation;

pub use validation::*;

use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    str::FromStr,
};

#[rustfmt::skip]
use ::protobuf::Message;
use anyhow::{anyhow, bail, Result};
use base58::FromBase58;
use borsh::{BorshDeserialize, BorshSerialize};
use bytes::{BufMut, Bytes};
use pwr_rs::wallet::PublicKey as PWRPublicKey;
use serde::{de::Visitor, Deserialize, Deserializer, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackID {
    PWRStackID(uuid::Uuid),
}

impl StackID {
    pub fn get_bytes(&self) -> &[u8; 16] {
        match self {
            Self::PWRStackID(key) => key.as_bytes(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(16 + 1);
        match self {
            Self::PWRStackID(key) => {
                res.push(2u8);
                res.put_slice(key.as_bytes());
            }
        }
        res
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 16 + 1 {
            bail!("Incorrect byte count");
        }

        match bytes[0] {
            // We already know we have exactly enough bytes, so it's safe to unwrap
            2u8 => Ok(Self::PWRStackID(
                uuid::Uuid::from_slice(&bytes[1..]).unwrap(),
            )),

            x => bail!("Unknown StackID discriminator {x}"),
        }
    }
}

impl Debug for StackID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PWRStackID(id) => {
                write!(f, "<PWR StackID (uuid): {}>", id)
            }
        }
    }
}

impl Display for StackID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PWRStackID(id) => write!(f, "p_{}", id),
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
                Ok(Self::PWRStackID(uuid::Uuid::from_slice(&bytes).map_err(
                    |_| ParseStackIDError::FailedToParse(anyhow!("Uuid length mismatch")),
                )?))
            }
            _ => Err(ParseStackIDError::UnknownVariant),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StackOwner {
    PWR(PWRPublicKey),
}

impl StackOwner {
    // TODO: violates multi-chain
    pub fn to_inner(&self) -> &PWRPublicKey {
        let Self::PWR(pk) = self;
        pk
    }

    pub fn from_bytes(bytes: [u8; 64]) -> Result<Self, ParseStackOwnerError> {
        PWRPublicKey::from_bytes(bytes)
            .map(Self::PWR)
            .map_err(|e| ParseStackOwnerError::FailedToParse(e.into()))
    }
}

impl Display for StackOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PWR(id) => write!(f, "p_{}", id),
        }
    }
}

#[derive(Error, Debug)]
pub enum ParseStackOwnerError {
    #[error("Invalid format")]
    InvalidFormat,

    #[error("Unknown variant")]
    UnknownVariant,

    #[error("Failed to parse: {0}")]
    FailedToParse(anyhow::Error),
}

impl FromStr for StackOwner {
    type Err = ParseStackOwnerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 3 || s.chars().nth(1) != Some('_') {
            return Err(ParseStackOwnerError::InvalidFormat);
        }

        let variant_code = s.chars().next();

        match variant_code {
            Some('p') => {
                let (_, code) = s.split_at(2);
                let pk = PWRPublicKey::from_str(code).map_err(|_| {
                    ParseStackOwnerError::FailedToParse(anyhow!(
                        "Failed to parse PWR public key string"
                    ))
                })?;
                Ok(Self::PWR(pk))
            }
            _ => Err(ParseStackOwnerError::UnknownVariant),
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
    #[allow(clippy::result_large_err)]
    pub fn validate(self) -> Result<ValidatedStack, (Self, StackValidationError)> {
        validate(self)
    }

    pub fn serialize_to_proto(self) -> Result<Bytes> {
        let stack: crate::protos::stack::Stack = self.into();
        Ok(stack.write_to_bytes()?.into())
    }

    pub fn try_deserialize_proto(bytes: impl AsRef<[u8]>) -> Result<Stack> {
        crate::protos::stack::Stack::parse_from_bytes(bytes.as_ref())?.try_into()
    }

    pub fn key_value_tables(&self) -> impl Iterator<Item = &NameAndDelete> {
        self.services.iter().filter_map(|s| match s {
            Service::KeyValueTable(x) => Some(x),
            _ => None,
        })
    }

    pub fn storages(&self) -> impl Iterator<Item = &NameAndDelete> {
        self.services.iter().filter_map(|s| match s {
            Service::Storage(x) => Some(x),
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
    KeyValueTable(NameAndDelete),
    Storage(NameAndDelete),
    Gateway(Gateway),
    Function(Function),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NameAndDelete {
    pub name: String,
    pub delete: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Gateway {
    pub name: String,
    pub endpoints: HashMap<String, HashMap<HttpMethod, AssemblyAndFunction>>,
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
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
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
