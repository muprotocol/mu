use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Response<'a> {
    pub body: Cow<'a, [u8]>,
}
