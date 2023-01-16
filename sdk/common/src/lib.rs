pub mod incoming_message;
pub mod outgoing_message;
mod response_builder;
mod status;

pub use response_builder::ResponseBuilder;
pub use status::Status;

use std::{borrow::Cow, collections::HashMap, hash::Hash};

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

// We only compare header names !!
impl<'a> PartialEq for Header<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl<'a> Eq for Header<'a> {}

impl<'a> Hash for Header<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Request<'a> {
    pub method: HttpMethod,
    pub path: Cow<'a, str>,
    pub query: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub headers: Vec<Header<'a>>,
    pub body: Cow<'a, [u8]>,
}

impl<'a> Request<'a> {
    pub fn content_type(&self) -> Option<Cow<'a, str>> {
        self.headers.iter().find_map(|header| {
            if &header.name.to_lowercase() == "content-type" {
                Some(header.value.clone())
            } else {
                None
            }
        })
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Response<'a> {
    pub status: Status,
    pub headers: Vec<Header<'a>>,
    pub body: Cow<'a, [u8]>,
}

impl<'a> Response<'a> {
    /// Create a [`ResponseBuilder`]
    pub fn build() -> ResponseBuilder<'a> {
        ResponseBuilder::default()
    }
}
