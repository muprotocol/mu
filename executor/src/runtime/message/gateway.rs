//TODO
#![allow(dead_code)]

use std::{borrow::Cow, collections::HashMap};

use super::{FromMessage, Message, ToMessage};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

#[derive(Serialize, Debug)]
pub struct Header<'a> {
    pub name: Cow<'a, str>,
    pub value: Cow<'a, str>,
}

#[derive(Serialize, Debug)]
pub struct Request<'a> {
    pub method: HttpMethod,
    pub path: &'a str,
    pub query: HashMap<&'a str, &'a str>,
    pub headers: Vec<Header<'a>>,
    pub data: &'a str,
}

#[derive(Deserialize, Debug)]
pub struct OwnedHeader {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<OwnedHeader>,
    pub body: String,
}

#[derive(Debug)]
pub struct GatewayRequest<'a> {
    request: Request<'a>,
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
    pub fn new(request: Request<'a>) -> Self {
        GatewayRequest { request }
    }
}

#[derive(Deserialize, Debug)]
pub struct GatewayResponse {
    pub response: Response,
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
