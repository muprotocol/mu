use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::any::type_name;
use uuid::Uuid;

use crate::runtime::error::Error;

use super::message::{Input, InputMessage, OutputMessage};

//TODO: change type acording to gateway actual request
#[derive(Serialize, Debug)]
pub struct GatewayRequest {
    id: Uuid,
    request: String,
}

impl Input for GatewayRequest {}

impl GatewayRequest {
    pub fn new(id: Uuid, request: String) -> Self {
        GatewayRequest { id, request }
    }

    pub fn from_input_message(input: InputMessage) -> Result<Self> {
        if input.get_type() == type_name::<Self>() {
            InputMessage::new_with_id(input.id, input.message)
        } else {
            bail!(Error::IncorrectInputMessage(type_name::<Self>()))
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct GatewayResponse {
    id: Uuid,
    response: String,
}

impl GatewayResponse {
    pub fn parse(message: OutputMessage) -> Result<Self> {
        let r#type = type_name::<Self>();
        if message.r#type == r#type {
            Ok(Self {
                id: message.id,
                response: serde_json::from_str(&message.message)?,
            })
        } else {
            bail!("can not deserialize as {}.", r#type)
        }
    }
}

#[derive(Deserialize)]
pub struct Log {
    category: String,
    r#type: LogType,
    content: String,
}

#[derive(Deserialize)]
pub enum LogType {
    Error,
    Debug,
    //TODO: add more log types here
}
