use std::{
    borrow::{Borrow, Cow},
    io::Cursor,
};

use borsh::BorshDeserialize;

use super::{FromPacket, PacketType};

#[derive(Debug, BorshDeserialize)]
pub struct Log {
    pub body: String, //TODO: use &str if can
}

impl<'a> FromPacket<'a> for Log {
    const TYPE: super::PacketType = PacketType::Log;

    fn from_bytes(bytes: Cow<'a, [u8]>) -> Result<Self, std::io::Error> {
        let mut cursor: Cursor<&[u8]> = Cursor::new(bytes.borrow());
        BorshDeserialize::deserialize_reader(&mut cursor)
    }
}
