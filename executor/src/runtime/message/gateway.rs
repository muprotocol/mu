//TODO
#![allow(dead_code)]

use crate::gateway;

use super::{FromMessage, Message, ToMessage};
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug)]
pub struct GatewayRequest<'a> {
    request: gateway::Request<'a>,
}

impl<'a> ToMessage for GatewayRequest<'a> {
    const TYPE: &'static str = "GatewayRequest";

    fn to_message(&self) -> Result<Message> {
        Ok(Message {
            id: None,
            r#type: Self::TYPE.to_owned(),
            // TODO: not good, why force user to only send JSON to functions?
            message: serde_json::to_value(&self.request)
                .context("gateway request serialization failed")?,
        })
    }
}

impl<'a> GatewayRequest<'a> {
    pub fn new(request: gateway::Request<'a>) -> Self {
        GatewayRequest { request }
    }
}

#[derive(Deserialize, Debug)]
pub struct GatewayResponse {
    pub response: gateway::Response,
}

impl FromMessage for GatewayResponse {
    const TYPE: &'static str = "GatewayResponse";

    fn from_message(m: Message) -> Result<Self> {
        Ok(Self {
            response: serde_json::from_value(m.message)
                .context("gateway response deserialization failed")?,
        })
    }
}
