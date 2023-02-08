pub use super::super::OptionValue;
use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct EmptyResult;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct SingleResult<'a> {
    pub item: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct ListResult<'a> {
    pub items: Vec<Cow<'a, [u8]>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TkvTriple<'a> {
    pub table: Cow<'a, str>,
    pub key: Cow<'a, [u8]>,
    pub value: Cow<'a, [u8]>,
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
pub struct TkvTriplesResult<'a> {
    pub tkv_triples: Vec<TkvTriple<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct CasResult<'a> {
    pub previous_value: OptionValue<Cow<'a, [u8]>>,
    pub is_swapped: bool,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct DbError<'a> {
    pub error: Cow<'a, str>,
}
