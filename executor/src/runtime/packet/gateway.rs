use borsh::{BorshDeserialize, BorshSerialize};

use super::{FromPacket, IntoPacket, PacketType};
use crate::gateway;

#[derive(Debug, BorshSerialize)]
pub struct Request<'a>(pub gateway::Request<'a>);

#[derive(Debug, BorshDeserialize)]
pub struct Response(pub gateway::Response);

impl<'a> IntoPacket for Request<'a> {
    const TYPE: PacketType = PacketType::GatewayRequest;

    fn as_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        self.try_to_vec()
    }
}

impl FromPacket for Response {
    const TYPE: PacketType = PacketType::GatewayResponse;

    fn from_bytes(bytes: &mut &[u8]) -> Result<Self, std::io::Error> {
        BorshDeserialize::deserialize_reader(bytes)
    }
}
