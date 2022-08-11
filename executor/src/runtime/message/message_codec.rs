use bytes::BufMut;
use bytes::{Buf, BytesMut};
use std::{cmp, fmt, io, usize};
use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

use super::message::FuncInput;
use super::message::Message;

/// A simple [`Decoder`] and [`Encoder`] implementation that splits up data into JSON objects.
///
/// [`Decoder`]: tokio_util::codec::Decoder
/// [`Encoder`]: tokio_util::codec::Encoder
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MessageCodec {
    // Stored index of the next index to examine for matching pairs of `{` and `}` character.
    // This is used to optimize searching.
    // For example, if `decode` was called with `{ "id": `, it would hold `8`,
    // because that is the next index to examine.
    // The next time `decode` is called with `{ "id": 1234 }`, the method will
    // only look at `1234 }` before returning.
    next_index: usize,

    /// The maximum length for a given object. If `usize::MAX`, objects will be
    /// read until maching pairs is found.
    max_length: usize,

    /// The stack holding pairs of braces to check json object boundries.
    current_pairs: Vec<char>,
}

impl MessageCodec {
    /// Returns a `MessageCodec` for splitting up data into messages.
    ///
    /// # Note
    ///
    /// The returned `MessageCodec` will not have an upper bound on the length
    /// of a buffered line. See the documentation for [`new_with_max_length`]
    /// for information on why this could be a potential security risk.
    ///
    /// [`new_with_max_length`]: crate::runtime::message::MessageCodec::new_with_max_length()
    pub fn new() -> Self {
        Self {
            next_index: 0,
            max_length: usize::MAX,
            current_pairs: Vec::new(),
        }
    }

    /// Returns a `MessageCodec` with a maximum message length limit.
    ///
    /// If this is set, calls to `MessageCodec::decode` will return a
    /// [`MessageCodecError`] when a Message exceeds the length limit. Subsequent calls
    /// will discard up to `limit` bytes from that Message until a newline
    /// character is reached, returning `None` until the line over the limit
    /// has been fully discarded. After that point, calls to `decode` will
    /// function as normal.
    ///
    /// # Note
    ///
    /// Setting a length limit is highly recommended for any `MessageCodec` which
    /// will be exposed to untrusted input. Otherwise, the size of the buffer
    /// that holds the message currently being read is unbounded. An attacker could
    /// exploit this unbounded buffer by sending an unbounded amount of input
    /// without any correct `}` characters, causing unbounded memory consumption.
    ///
    /// [`MessageCodecError`]: crate::runtime::message::MessageCodecError
    pub fn new_with_max_length(max_length: usize) -> Self {
        Self {
            max_length,
            ..MessageCodec::new()
        }
    }

    pub fn max_length(&self) -> usize {
        self.max_length
    }
}

impl Decoder for MessageCodec {
    type Item = Result<Message, Self::Error>;
    type Error = MessageCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        'main_loop: loop {
            // Determine how far into the buffer we'll search. If
            // there's no max_length set, we'll read to the end of the buffer.
            let read_to = cmp::min(self.max_length.saturating_add(1), buf.len());

            if let Some(cnt) = buf[self.next_index..read_to]
                .iter()
                .position(|b| *b == b'{')
            {
                if cnt != 0 {
                    buf.advance(cnt);
                    self.next_index = 0;
                    if buf.is_empty() {
                        return Ok(None);
                    }
                    continue 'main_loop;
                }
            }

            for (idx, ch) in buf[self.next_index..read_to].iter().enumerate() {
                match ch {
                    b'{' => self.current_pairs.push('{'),
                    b'}' => match self.current_pairs.pop() {
                        Some('{') => {
                            if self.current_pairs.is_empty() {
                                self.next_index = 0;

                                let bytes = buf.split_to(idx + 1);
                                return Ok(Some(
                                    serde_json::from_slice(&bytes).map_err(Self::Error::Decoding),
                                ));
                            }
                        }

                        // Invalid Data found
                        _ => {
                            self.current_pairs.clear();
                            self.next_index = 0;
                            buf.advance(self.next_index + idx + 1);
                            return Ok(Some(Err(Self::Error::InvalidBytes)));
                        }
                    },
                    _ => (),
                }
            }

            if buf.len() > self.max_length {
                // Reached the maximum length without finding a
                // newline, return an error
                return Err(MessageCodecError::MaxMessageLengthExceeded);
            } else {
                // We didn't find a message or reach the length limit, so the next
                // call will resume searching at the current offset.
                self.next_index = read_to;
                return Ok(None);
            }
        }
    }
}

impl Encoder<Message> for MessageCodec {
    type Error = MessageCodecError;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let bytes = serde_json::to_string(&item).map_err(|e| Self::Error::Encoding(e.into()))?;
        let bytes = bytes.as_bytes();
        dst.reserve(bytes.len());
        dst.put(bytes);
        Ok(())
    }
}

impl Default for MessageCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// An error occurred while encoding or decoding a message.
#[derive(Debug)]
pub enum MessageCodecError {
    /// The maximum message length was exceeded.
    MaxMessageLengthExceeded,
    /// An IO error occurred.
    Io(io::Error),
    /// Encoding error
    Encoding(anyhow::Error),
    /// Serde decoding error
    Decoding(serde_json::Error),
    /// Invalid bytes while decoding
    InvalidBytes,
}

impl fmt::Display for MessageCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageCodecError::MaxMessageLengthExceeded => write!(f, "max message length exceeded"),
            MessageCodecError::Io(e) => write!(f, "{}", e),
            MessageCodecError::Encoding(e) => write!(f, "{}", e),
            MessageCodecError::Decoding(e) => write!(f, "{}", e),
            MessageCodecError::InvalidBytes => write!(f, "invalid bytes found"),
        }
    }
}

impl From<io::Error> for MessageCodecError {
    fn from(e: io::Error) -> MessageCodecError {
        MessageCodecError::Io(e)
    }
}

impl std::error::Error for MessageCodecError {}
