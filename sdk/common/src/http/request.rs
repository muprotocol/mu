use core::fmt;
use std::{borrow::Cow, collections::HashMap};

use borsh::{BorshDeserialize, BorshSerialize};

use super::{Body, Header, HttpMethod, Url, Version};

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Request<'a> {
    pub method: HttpMethod,
    pub url: Url,
    path_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
    query_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub headers: Vec<Header<'a>>,
    pub body: Option<Body<'a>>,
    pub version: Version,
}

impl<'a> Request<'a> {
    /// Constructs a new request.
    #[inline]
    pub fn new(method: HttpMethod, url: Url) -> Self {
        Request {
            method,
            path_params: HashMap::new(),
            query_params: HashMap::new(),
            url,
            headers: vec![],
            body: None,
            version: Version::default(),
        }
    }

    /// Get the content-type header otherwise None if there is no content-type header.
    pub fn content_type(&self) -> Option<Cow<'a, str>> {
        self.headers.iter().find_map(|header| {
            if &header.name.to_lowercase() == "content-type" {
                Some(header.value.clone())
            } else {
                None
            }
        })
    }

    /// Get the method.
    #[inline]
    pub fn method(&self) -> &HttpMethod {
        &self.method
    }

    /// Get the url.
    #[inline]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the headers.
    #[inline]
    pub fn headers(&self) -> &Vec<Header<'a>> {
        &self.headers
    }

    /// Get the body.
    #[inline]
    pub fn body(&self) -> Option<&Body<'a>> {
        self.body.as_ref()
    }

    /// Get the path params.
    #[inline]
    pub fn path_params(&self) -> &HashMap<Cow<'a, str>, Cow<'a, str>> {
        &self.path_params
    }

    /// Get the query params.
    #[inline]
    pub fn query_params(&self) -> &HashMap<Cow<'a, str>, Cow<'a, str>> {
        &self.query_params
    }

    /// Get the http version.
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }
}

impl<'a> fmt::Debug for Request<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_request_fields(&mut f.debug_struct("Request"), self).finish()
    }
}

fn fmt_request_fields<'a, 'b>(
    f: &'a mut fmt::DebugStruct<'a, 'b>,
    req: &Request,
) -> &'a mut fmt::DebugStruct<'a, 'b> {
    f.field("method", &req.method)
        .field("url", &req.url)
        .field("headers", &req.headers)
}
