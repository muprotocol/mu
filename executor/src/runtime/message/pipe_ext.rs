use super::{message_codec::MessageCodec, MessageReader, MessageWriter};
use std::{
    io::{BufReader, BufWriter, Read, Write},
    pin::Pin,
    task::Poll,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite};
use wasmer_wasi::Pipe;

pub struct AsyncReadPipe(BufReader<Pipe>);
pub struct AsyncWritePipe(BufWriter<Pipe>);

impl AsyncReadPipe {
    fn pin_buffer(self: Pin<&mut Self>) -> &mut BufReader<Pipe> {
        // This is okay because field is never considered pinned.
        unsafe { &mut self.get_unchecked_mut().0 }
    }
}

impl AsyncWritePipe {
    fn pin_buffer(self: Pin<&mut Self>) -> &mut BufWriter<Pipe> {
        // This is okay because field is never considered pinned.
        unsafe { &mut self.get_unchecked_mut().0 }
    }
}

impl From<Pipe> for AsyncReadPipe {
    fn from(p: Pipe) -> Self {
        Self(BufReader::new(p))
    }
}

impl From<Pipe> for AsyncWritePipe {
    fn from(p: Pipe) -> Self {
        Self(BufWriter::new(p))
    }
}

// TODO: find a way to do REALLY async read
impl AsyncRead for AsyncReadPipe {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.pin_buffer().read(buf.initialized_mut()) {
            Ok(0) => Poll::Pending, // will hang forever
            Ok(_) => Poll::Ready(Ok(())),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

// TODO: find a way to do REALLY async write
impl AsyncWrite for AsyncWritePipe {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Poll::Ready(self.pin_buffer().write(buf))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(self.pin_buffer().flush())
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }
}

pub trait PipeExt {
    fn to_message_reader(self) -> MessageReader;
    fn to_message_writer(self) -> MessageWriter;
}

impl PipeExt for Pipe {
    fn to_message_reader(self) -> MessageReader {
        FramedRead::new(
            AsyncReadPipe::from(self),
            MessageCodec::new_with_max_length(super::MAX_MESSAGE_LEN),
        )
    }

    fn to_message_writer(self) -> MessageWriter {
        FramedWrite::new(
            AsyncWritePipe::from(self),
            MessageCodec::new_with_max_length(super::MAX_MESSAGE_LEN),
        )
    }
}
