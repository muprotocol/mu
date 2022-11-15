//TODO
#![allow(dead_code)]

pub mod database;
pub mod gateway;
pub mod log;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::error::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: Option<u64>,
    pub r#type: String,
    pub message: serde_json::Value,
}

impl Message {
    pub fn as_bytes(self) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(&self).map_err(|e| Error::MessageSerializationFailed(e))
    }
}

pub trait ToMessage {
    const TYPE: &'static str;

    fn to_message(&self) -> Result<Message, Error>;
}

pub trait FromMessage: Sized {
    const TYPE: &'static str;

    fn from_message(m: Message) -> Result<Self, Error>;
}
