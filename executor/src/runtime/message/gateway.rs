//TODO
#![allow(dead_code)]

use std::any::type_name;

use crate::runtime::function::FunctionID;

use super::{FuncInput, FuncOutput, Message};
use anyhow::Result;
use serde::{Deserialize, Serialize};

// TODO: Change type based on actual gateway request
#[derive(Serialize, Debug)]
pub struct GatewayRequest {
    id: u64,
    pub function_id: FunctionID,
    request: String,
}

impl FuncInput for GatewayRequest {
    const TYPE: &'static str = "GatewayRequest";

    fn to_message(&self) -> Result<Message> {
        Ok(Message {
            id: self.id,
            r#type: type_name::<Self>().to_owned(),
            message: serde_json::to_value(&self.request)?,
        })
    }
}

impl GatewayRequest {
    pub fn new(id: u64, function_id: FunctionID, request: String) -> Self {
        GatewayRequest {
            id,
            function_id,
            request,
        }
    }
}

// TODO: Change type based on actual gateway response
#[derive(Deserialize, Debug)]
pub struct GatewayResponse {
    id: u64,
    response: String,
}

impl<'a> FuncOutput<'a> for GatewayResponse {
    const TYPE: &'static str = "GatewayResponse";

    fn from_message(m: Message) -> Result<Self> {
        Ok(Self {
            id: m.id,
            response: serde_json::from_value(m.message)?,
        })
    }
}
