use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

pub const AUTHORIZATION_HEADER: &str = "authorization";
pub const CONTENT_TYPE_HEADER: &str = "content-type";

pub const BINARY_CONTENT_TYPE: &str = "application/octet-stream";
pub const STRING_CONTENT_TYPE: &str = "text/plain; charset=utf-8";

#[derive(Debug, BorshSerialize, BorshDeserialize, Clone)]
pub struct Header<'a> {
    pub name: Cow<'a, str>,
    pub value: Cow<'a, str>,
}
