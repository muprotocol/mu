use std::{
    borrow::{Borrow, Cow},
    io::Cursor,
};

use borsh::{BorshDeserialize, BorshSerialize};

use super::{FromPacket, IntoPacket, PacketType};
use crate::gateway;

#[derive(Debug, BorshSerialize)]
pub struct Request<'a>(pub gateway::Request<'a>);

#[derive(Debug, BorshDeserialize)]
pub struct Response(pub gateway::Response);

impl<'a> IntoPacket<'a> for Request<'a> {
    const TYPE: PacketType = PacketType::GatewayRequest;

    fn as_bytes(&'a self) -> Result<Cow<'a, [u8]>, std::io::Error> {
        self.try_to_vec().map(Cow::Owned)
    }
}

impl<'a> FromPacket<'a> for Response {
    const TYPE: PacketType = PacketType::GatewayResponse;

    fn from_bytes(bytes: Cow<'a, [u8]>) -> Result<Self, std::io::Error> {
        let mut cursor: Cursor<&[u8]> = Cursor::new(bytes.borrow());
        BorshDeserialize::deserialize_reader(&mut cursor)
    }
}
