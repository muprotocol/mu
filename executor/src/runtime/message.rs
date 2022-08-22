//TODO
#![allow(dead_code)]

pub mod database;
pub mod gateway;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub r#type: String,
    pub message: serde_json::Value,
}

impl Message {
    pub fn as_bytes(self) -> Result<Vec<u8>> {
        serde_json::to_vec(&self).map_err(Into::into)
    }
}

pub trait ToMessage {
    const TYPE: &'static str;

    fn to_message(&self) -> Result<Message>;
}

pub trait FromMessage: Sized {
    const TYPE: &'static str;

    fn from_message(m: Message) -> Result<Self>;
}
