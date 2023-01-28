use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct SingleResult<'a> {
    pub value: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KVPair<'a> {
    pub key: Cow<'a, [u8]>,
    pub value: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct ListResult<'a> {
    pub kv_pairs: Vec<KVPair<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct DBError<'a> {
    pub error: Cow<'a, str>,
}
