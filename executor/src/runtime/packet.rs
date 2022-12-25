pub mod database;
pub mod gateway;
pub mod log;

use std::{borrow::Cow, fmt::Display};

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Packet<'a> {
    pub id: u64,
    data_type: PacketType,
    data: Cow<'a, [u8]>,
}

impl<'a> Packet<'a> {
    //TODO: use ToOwned trait
    pub fn to_owned(self) -> Packet<'static> {
        let data = self.data.to_vec();

        Packet {
            id: self.id,
            data_type: self.data_type,
            data: Cow::Owned(data),
        }
    }

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

pub trait IntoPacket<'a> {
    const TYPE: PacketType;

    fn as_bytes(&'a self) -> Result<Cow<'a, [u8]>, std::io::Error>;

    fn into_packet(&'a self, id: u64) -> Result<Packet<'a>, PacketError> {
        Ok(Packet {
            id,
            data_type: Self::TYPE,
            data: Self::as_bytes(&self).map_err(PacketError::IOError)?,
        })
    }
}

pub trait FromPacket<'a>: Sized {
    const TYPE: PacketType;

    fn from_bytes(bytes: Cow<'a, [u8]>) -> Result<Self, std::io::Error>;

    fn from_packet(packet: Packet<'a>) -> Result<Self, PacketError> {
        if packet.data_type != Self::TYPE {
            Err(PacketError::PacketTypeMismatch)
        } else {
            Self::from_bytes(packet.data).map_err(PacketError::IOError)
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
