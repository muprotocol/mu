use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct Request<'a> {
    pub path: Cow<'a, str>,
    pub body: Cow<'a, [u8]>,
}
