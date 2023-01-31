use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct EmptyResult;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct SingleResult<'a> {
    pub key_or_value: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KeyListResult<'a> {
    pub keys: Vec<Cow<'a, [u8]>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KvPair<'a> {
    pub key: Cow<'a, [u8]>,
    pub value: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KvPairsResult<'a> {
    pub kv_pairs: Vec<KvPair<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct DbError<'a> {
    pub error: Cow<'a, str>,
}
