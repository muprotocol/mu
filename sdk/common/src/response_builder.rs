use std::{borrow::Cow, collections::HashSet};

use crate::{Header, Response, Status};

pub struct ResponseBuilder<'a> {
    status: Status,
    headers: HashSet<Header<'a>>,
}

impl<'a> ResponseBuilder<'a> {
    pub fn new() -> Self {
        ResponseBuilder {
            status: Status::Ok,
            headers: HashSet::new(),
        }
    }

    pub fn status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    pub fn content_type(mut self, content_type: Cow<'a, str>) -> Self {
        let header = Header {
            name: Cow::Borrowed("content-type"),
            value: content_type,
        };

        if self.headers.contains(&header) {}
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
