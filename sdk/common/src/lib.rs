pub mod incoming_message;
pub mod outgoing_message;

use std::{borrow::Cow, collections::HashMap};

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Header<'a> {
    pub name: Cow<'a, str>,
    pub value: Cow<'a, str>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Request<'a> {
    pub method: HttpMethod,
    pub path: Cow<'a, str>,
    pub query: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub headers: Vec<Header<'a>>,
    pub body: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Response<'a> {
    pub status: u16,
    pub content_type: Cow<'a, str>,
    pub headers: Vec<Header<'a>>,
    pub body: Cow<'a, [u8]>,
}
