// parts of this file are derived from `reqwest` https://github.com/seanmonstar/reqwest
//
// Copyright (c) 2016 Sean McArthur
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

use core::fmt;
use std::{borrow::Cow, collections::HashMap};

use borsh::{BorshDeserialize, BorshSerialize};

use super::{Body, Header, HttpMethod, Url, Version};

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct Request<'a> {
    pub method: HttpMethod,
    pub url: Url,
    pub path_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub query_params: HashMap<Cow<'a, str>, Cow<'a, str>>,
    pub headers: Vec<Header<'a>>,
    pub body: Body<'a>,
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
            body: Cow::Borrowed(&[]),
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
