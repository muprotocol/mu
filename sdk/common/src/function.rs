mod response;

use std::{borrow::Cow, collections::HashMap};

use borsh::{BorshDeserialize, BorshSerialize};

pub use crate::common_http::{Header, HttpMethod, Status};
pub use response::{Response, ResponseBuilder};

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

    pub fn x_correlation_id(&self) -> Option<Cow<'a, str>> {
        self.headers
            .iter()
            .find(|h| h.name.to_lowercase() == "x-correlation-id")
            .map(|h| h.value.clone())
    }
}
