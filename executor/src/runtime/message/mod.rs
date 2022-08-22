//TODO
#![allow(dead_code)]

pub mod database;
pub mod gateway;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// TODO: move to configs: default 8k
pub const MAX_MESSAGE_LEN: usize = 1024 * 8;

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

pub trait FuncInput
where
    Self: Serialize,
{
    const TYPE: &'static str;

    fn to_message(&self) -> Result<Message>;
}

pub trait FuncOutput<'a>
where
    Self: Deserialize<'a>,
{
    const TYPE: &'static str;

    fn from_message(m: Message) -> Result<Self>;
}
