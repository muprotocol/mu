use super::pipe_ext::{AsyncReadPipe, AsyncWritePipe};
use anyhow::Result;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{any::type_name, io::Write};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use wasmer_wasi::Pipe;

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub r#type: String,
    pub message: String,
}

impl Message {
    pub fn to_writer<W: Write>(&self, writer: W) -> Result<()> {
        serde_json::to_writer(writer, self).map_err(Into::into)
    }
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

pub struct MessageReader(pub FramedRead<AsyncReadPipe, LinesCodec>);

impl MessageReader {
    pub fn new(pipe: Pipe) -> Self {
        let ap = AsyncReadPipe::from(pipe);
        Self(FramedRead::new(ap, LinesCodec::new()))
    }
}

pub struct MessageWriter(FramedWrite<AsyncWritePipe, LinesCodec>);

impl MessageWriter {
    pub fn new(pipe: Pipe) -> Self {
        let ap = AsyncWritePipe::from(pipe);
        Self(FramedWrite::new(ap, LinesCodec::new()))
    }
}
