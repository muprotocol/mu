pub mod database;
pub mod gateway;
pub mod log;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize)]
pub struct InputPacket<'a> {
    pub id: u64,
    pub message: InputMessage<'a>,
}

#[derive(BorshSerialize)]
pub enum InputMessage<'a> {
    Request(gateway::Request<'a>),
    DbResponse(database::Response),
}

#[derive(BorshDeserialize)]
pub struct OutputPacket {
    pub id: u64,
    pub message: OutputMessage,
}

#[derive(BorshDeserialize)]
pub enum OutputMessage {
    Response(gateway::Response),
    DbRequest(database::Request),
    Log(log::Log),
}
