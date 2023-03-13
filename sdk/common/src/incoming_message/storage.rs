use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct StorageEmptyResult;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Object<'a> {
    pub key: Cow<'a, str>,
    pub size: u64,
}
#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct ObjectListResult<'a> {
    pub list: Vec<Object<'a>>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct StorageError<'a> {
    pub error: Cow<'a, str>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct StorageGetResult<'a> {
    pub data: Cow<'a, [u8]>,
}
