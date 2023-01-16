use std::{borrow::Cow, collections::HashMap};

use crate::{Header, Response, Status};

const CONTENT_TYPE_HEADER: &str = "content-type";
const BINARY_CONTENT_TYPE: &str = "application/octet-stream";
const STRING_CONTENT_TYPE: &str = "text/plain; charset=utf-8";

pub struct ResponseBuilder<'a> {
    status: Status,
    headers: HashMap<Cow<'a, str>, Header<'a>>,
}

impl<'a> ResponseBuilder<'a> {
    pub fn new() -> Self {
        ResponseBuilder {
            status: Status::Ok,
            headers: HashMap::new(),
        }
    }

    pub fn status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    pub fn content_type(mut self, content_type: Cow<'a, str>) -> Self {
        let header = Header {
            name: Cow::Borrowed(CONTENT_TYPE_HEADER),
            value: content_type,
        };

        self.headers.remove(&header.name);
        self.headers.insert(header.name.clone(), header);
        self
    }

    fn has_content_type(&self) -> bool {
        self.headers
            .contains_key(&Cow::Borrowed(CONTENT_TYPE_HEADER))
    }

    /// Adds a [`Header`] to response and overrides the header if already exists.
    pub fn header(mut self, header: Header<'a>) -> Self {
        let name: Cow<'a, str> = header.name.to_lowercase().into();
        self.headers.remove(&name);
        self.headers.insert(name, header);
        self
    }

    pub fn headers(self, headers: Vec<Header<'a>>) -> Self {
        headers.into_iter().fold(self, Self::header)
    }

    pub fn body_from_slice(mut self, slice: &'a [u8]) -> Response<'a> {
        if !self.has_content_type() {
            self = self.content_type(Cow::Borrowed(BINARY_CONTENT_TYPE));
        }

        Response {
            status: self.status,
            headers: self.headers.into_values().collect(),
            body: Cow::Borrowed(slice),
        }
    }

    pub fn body_from_vec(mut self, vec: Vec<u8>) -> Response<'a> {
        if !self.has_content_type() {
            self = self.content_type(Cow::Borrowed(BINARY_CONTENT_TYPE));
        }

        Response {
            status: self.status,
            headers: self.headers.into_values().collect(),
            body: Cow::Owned(vec),
        }
    }

    pub fn body_from_string(mut self, string: String) -> Response<'a> {
        if !self.has_content_type() {
            self = self.content_type(Cow::Borrowed(STRING_CONTENT_TYPE));
        }

        Response {
            status: self.status,
            headers: self.headers.into_values().collect(),
            body: Cow::Owned(string.as_bytes().to_vec()),
        }
    }

    pub fn body_from_str(mut self, str: &'a str) -> Response<'a> {
        if !self.has_content_type() {
            self = self.content_type(Cow::Borrowed(STRING_CONTENT_TYPE));
        }

        Response {
            status: self.status,
            headers: self.headers.into_values().collect(),
            body: Cow::Borrowed(str.as_bytes()),
        }
    }
}

impl<'a> Default for ResponseBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}
