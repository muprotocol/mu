use std::str::FromStr;

use crate::StackOwner;

use super::StackID;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize_stack_id<S>(item: &StackID, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let s = item.to_string();
    serializer.serialize_str(&s)
}

pub fn deserialize_stack_id<'de, D>(deserializer: D) -> Result<StackID, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    StackID::from_str(&s).map_err(|_| serde::de::Error::custom("invalid StackID"))
}

pub fn serialize_stack_owner<S>(item: &StackOwner, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let s = item.to_string();
    serializer.serialize_str(&s)
}

pub fn deserialize_stack_owner<'de, D>(deserializer: D) -> Result<StackOwner, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    StackOwner::from_str(&s).map_err(|_| serde::de::Error::custom("invalid StackOwner"))
}
