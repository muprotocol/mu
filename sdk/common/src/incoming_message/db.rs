use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

type Key<'a> = Cow<'a, [u8]>;
type Value<'a> = Cow<'a, [u8]>;
type KeyOrValue<'a> = Cow<'a, [u8]>;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct EmptyResult;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct SingleResult<'a> {
    pub item: KeyOrValue<'a>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct ListResult<'a> {
    pub keys: Vec<Key<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KvPair<'a> {
    pub key: Key<'a>,
    pub value: Value<'a>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KvPairsResult<'a> {
    pub kv_pairs: Vec<KvPair<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct DbError<'a> {
    pub error: Cow<'a, str>,
}
