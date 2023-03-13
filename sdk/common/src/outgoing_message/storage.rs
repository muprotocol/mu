use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct StorageGet<'a> {
    pub storage_name: Cow<'a, str>,
    pub key: Cow<'a, str>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct StoragePut<'a> {
    pub storage_name: Cow<'a, str>,
    pub key: Cow<'a, str>,
    pub reader: Cow<'a, [u8]>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct StorageDelete<'a> {
    pub storage_name: Cow<'a, str>,
    pub key: Cow<'a, str>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct StorageList<'a> {
    pub storage_name: Cow<'a, str>,
    pub prefix: Cow<'a, str>,
}
