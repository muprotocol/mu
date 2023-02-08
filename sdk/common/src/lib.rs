pub mod incoming_message;
pub mod outgoing_message;
mod response_builder;
mod status;

pub use response_builder::ResponseBuilder;
pub use status::Status;

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
    pub path_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub query_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
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
    pub fn builder() -> ResponseBuilder<'a> {
        ResponseBuilder::default()
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct OptionValue<T> {
    pub is_some: bool,
    pub some: T,
}

impl<T: Default> From<Option<T>> for OptionValue<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(some) => Self {
                is_some: true,
                some,
            },
            None => Self {
                is_some: false,
                some: T::default(),
            },
        }
    }
}

impl<T> From<OptionValue<T>> for Option<T> {
    fn from(value: OptionValue<T>) -> Self {
        if value.is_some {
            Some(value.some)
        } else {
            None
        }
    }
}
