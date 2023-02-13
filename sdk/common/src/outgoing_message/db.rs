use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Put<'a> {
    pub table: Cow<'a, [u8]>,
    pub key: Cow<'a, [u8]>,
    pub value: Cow<'a, [u8]>,
    pub is_atomic: bool,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Get<'a> {
    pub table: Cow<'a, [u8]>,
    pub key: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Delete<'a> {
    pub table: Cow<'a, [u8]>,
    pub key: Cow<'a, [u8]>,
    pub is_atomic: bool,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct DeleteByPrefix<'a> {
    pub table: Cow<'a, [u8]>,
    pub key_prefix: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Scan<'a> {
    pub table: Cow<'a, [u8]>,
    pub key_prefix: Cow<'a, [u8]>,
    pub limit: u32,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct ScanKeys<'a> {
    pub table: Cow<'a, [u8]>,
    pub key_prefix: Cow<'a, [u8]>,
    pub limit: u32,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct CompareAndSwap<'a> {
    pub table: Cow<'a, [u8]>,
    pub key: Cow<'a, [u8]>,
    pub new_value: Cow<'a, [u8]>,
    pub previous_value: Option<Cow<'a, [u8]>>,
}

type TableName<'a> = Cow<'a, [u8]>;
type Key<'a> = Cow<'a, [u8]>;
type Value<'a> = Cow<'a, [u8]>;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchPut<'a> {
    pub table_key_value_triples: Vec<(TableName<'a>, Key<'a>, Value<'a>)>,
    pub is_atomic: bool,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchGet<'a> {
    pub table_key_tuples: Vec<(TableName<'a>, Key<'a>)>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchDelete<'a> {
    pub table_key_tuples: Vec<(TableName<'a>, Key<'a>)>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchScan<'a> {
    pub table_key_prefix_tuples: Vec<(TableName<'a>, Key<'a>)>,
    pub each_limit: u32,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchScanKeys<'a> {
    pub table_key_prefix_tuples: Vec<(TableName<'a>, Key<'a>)>,
    pub each_limit: u32,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableList<'a> {
    pub table_prefix: Cow<'a, [u8]>,
}
