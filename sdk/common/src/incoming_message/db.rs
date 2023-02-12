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
    pub list: Vec<Cow<'a, [u8]>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableKeyValue<'a> {
    pub table: Cow<'a, str>,
    pub key: Cow<'a, [u8]>,
    pub value: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableKey<'a> {
    pub table: Cow<'a, str>,
    pub key: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KeyValue<'a> {
    pub key: Cow<'a, [u8]>,
    pub value: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct KeyValueListResult<'a> {
    pub list: Vec<KeyValue<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableKeyListResult<'a> {
    pub list: Vec<TableKey<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableKeyValueListResult<'a> {
    pub list: Vec<TableKeyValue<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct CasResult<'a> {
    pub previous_value: Option<Cow<'a, [u8]>>,
    pub is_swapped: bool,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct DbError<'a> {
    pub error: Cow<'a, str>,
}
