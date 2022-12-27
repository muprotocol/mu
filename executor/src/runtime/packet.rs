pub mod database;
pub mod gateway;
pub mod log;

use std::fmt::Display;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Packet {
    data_type: PacketType,
    data: Vec<u8>,
}

impl Packet {
    pub fn data_type(&self) -> PacketType {
        self.data_type
    }
}

/// Order is important!, don't move.
#[derive(Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq, Clone, Copy)]
pub enum PacketType {
    GatewayRequest = 0,
    GatewayResponse,
    Log,
    DbRequest,
    DbResponse,
}

pub trait IntoPacket {
    const TYPE: PacketType;

    fn as_bytes(&self) -> Result<Vec<u8>, std::io::Error>;

    fn into_packet(&self) -> Result<Packet, PacketError> {
        Ok(Packet {
            data_type: Self::TYPE,
            data: Self::as_bytes(&self).map_err(PacketError::IOError)?,
        })
    }
}

pub trait FromPacket: Sized {
    const TYPE: PacketType;

    fn from_bytes(bytes: &mut &[u8]) -> Result<Self, std::io::Error>;

    fn from_packet(packet: Packet) -> Result<Self, PacketError> {
        if packet.data_type != Self::TYPE {
            Err(PacketError::PacketTypeMismatch)
        } else {
            Self::from_bytes(&mut packet.data.as_slice()).map_err(PacketError::IOError)
        }
    }
}

#[derive(Debug)]
pub enum PacketError {
    IOError(std::io::Error),
    PacketTypeMismatch,
}

impl Display for PacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PacketError::IOError(e) => format!("io error: {e}"),
            PacketError::PacketTypeMismatch => "packet and data type mismatch".into(),
        }
        .fmt(f)
    }
}
