//TODO
#![allow(dead_code)]

use crate::{gateway, runtime::error::Error};

use super::{FromMessage, Message, ToMessage};
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug)]
pub struct GatewayRequest<'a> {
    request: gateway::Request<'a>,
}

impl<'a> ToMessage for GatewayRequest<'a> {
    const TYPE: &'static str = "GatewayRequest";

    fn to_message(&self) -> Result<Message, Error> {
        Ok(Message {
            id: None,
            r#type: Self::TYPE.to_owned(),
            // TODO: not good, why force user to only send JSON to functions?
            message: serde_json::to_value(&self.request)
                .map_err(Error::MessageSerializationFailed)?,
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

    fn from_message(m: Message) -> Result<Self, Error> {
        Ok(Self {
            response: serde_json::from_value(m.message)
                .map_err(Error::MessageDeserializationFailed)?,
        })
    }
}
