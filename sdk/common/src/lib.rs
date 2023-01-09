pub mod incoming_message;
pub mod outgoing_message;
pub mod status;

use std::{borrow::Cow, collections::HashMap};

use borsh::{BorshDeserialize, BorshSerialize};
use status::Status;

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

pub struct ResponseBuilder<'a> {
    status: Status,
    headers: Vec<Header<'a>>,
    content_type: Cow<'a, str>,
}

impl<'a> ResponseBuilder<'a> {
    pub fn new() -> Self {
        ResponseBuilder {
            status: Status::Ok,
            headers: vec![],
            content_type: Cow::Borrowed("text/plain; charset=utf-8"),
        }
    }

    pub fn status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    pub fn content_type(mut self, content_type: Cow<'a, str>) -> Self {
        self.content_type = content_type;
        self
    }

    pub fn header(mut self, header: Header<'a>) -> Self {
        self.headers.push(header);
        self
    }

    pub fn headers(mut self, mut headers: Vec<Header<'a>>) -> Self {
        self.headers.append(&mut headers);
        self
    }
    pub fn body_from_slice(mut self, slice: &'a [u8]) -> Response<'a> {
        self.headers.push(Header {
            name: Cow::Borrowed("content-type"),
            value: self.content_type,
        });

        Response {
            status: self.status,
            headers: self.headers,
            body: Cow::Borrowed(slice),
        }
    }

    pub fn body_from_vec(mut self, vec: Vec<u8>) -> Response<'a> {
        self.headers.push(Header {
            name: Cow::Borrowed("content-type"),
            value: self.content_type,
        });

        Response {
            status: self.status,
            headers: self.headers,
            body: Cow::Owned(vec),
        }
    }
}

impl<'a> Default for ResponseBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}
