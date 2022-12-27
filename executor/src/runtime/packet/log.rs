use std::fmt::Display;

use borsh::BorshDeserialize;

use super::{FromPacket, PacketType};

#[derive(Debug, BorshDeserialize)]
pub struct Log {
    pub body: String,
}

impl Display for Log {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.body.fmt(f)
    }
}

impl FromPacket for Log {
    const TYPE: super::PacketType = PacketType::Log;

    fn from_bytes(bytes: &mut &[u8]) -> Result<Self, std::io::Error> {
        BorshDeserialize::deserialize_reader(bytes)
    }
}
