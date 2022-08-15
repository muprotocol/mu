//TODO
#![allow(dead_code)]

pub mod database;
pub mod gateway;
mod message_codec;
pub mod pipe_ext;

use anyhow::Result;
use message_codec::MessageCodec;
use pipe_ext::{AsyncReadPipe, AsyncWritePipe};
use serde::{Deserialize, Serialize};
use std::any::type_name;
use tokio_util::codec::{FramedRead, FramedWrite};

// TODO: move to configs: default 8k
pub const MAX_MESSAGE_LEN: usize = 1024 * 8;

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub r#type: String,
    pub message: serde_json::Value,
}

pub trait FuncInput
where
    Self: Serialize,
{
    fn get_type() -> String {
        type_name::<Self>().to_owned()
    }

    fn to_message(&self) -> Result<Message>;
}

pub trait FuncOutput<'a>
where
    Self: Deserialize<'a>,
{
    fn get_type() -> String {
        type_name::<Self>().to_owned()
    }

    fn from_message(m: Message) -> Result<Self>;
}

pub type MessageReader = FramedRead<AsyncReadPipe, MessageCodec>;

pub type MessageWriter = FramedWrite<AsyncWritePipe, MessageCodec>;
