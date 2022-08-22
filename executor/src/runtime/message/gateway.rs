//TODO
#![allow(dead_code)]

use super::{FromMessage, Message, ToMessage};
use anyhow::Result;
use serde::{Deserialize, Serialize};

// TODO: Change type based on actual gateway request
#[derive(Debug)]
pub struct GatewayRequest {
    id: u64,
    request: GatewayRequestDetails,
}

// TODO: completely unsuitable!
#[derive(Serialize, Debug)]
pub struct GatewayRequestDetails {
    pub local_path_and_query: String,
    pub body: String,
}

impl ToMessage for GatewayRequest {
    const TYPE: &'static str = "GatewayRequest";

    fn to_message(&self) -> Result<Message> {
        Ok(Message {
            id: self.id,
            r#type: Self::TYPE.to_owned(),
            // TODO: not good, why force user to only send JSON to functions?
            message: serde_json::to_value(&self.request)?,
        })
    }
}

impl GatewayRequest {
    pub fn new(id: u64, request: GatewayRequestDetails) -> Self {
        GatewayRequest { id, request }
    }
}

// TODO: Change type based on actual gateway response
#[derive(Deserialize, Debug)]
pub struct GatewayResponse {
    id: u64,
    pub response: String,
}

impl FromMessage for GatewayResponse {
    const TYPE: &'static str = "GatewayResponse";

    fn from_message(m: Message) -> Result<Self> {
        Ok(Self {
            id: m.id,
            response: serde_json::from_value(m.message)?,
        })
    }
}
