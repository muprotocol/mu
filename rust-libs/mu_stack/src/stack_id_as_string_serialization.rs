use std::str::FromStr;

use super::StackID;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S>(item: &StackID, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let s = item.to_string();
    serializer.serialize_str(&s)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<StackID, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    StackID::from_str(&s).map_err(|_| serde::de::Error::custom("invalid StackID"))
}
