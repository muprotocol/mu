use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

type Key<'a> = Cow<'a, [u8]>;
type Value<'a> = Cow<'a, [u8]>;
type TableName<'a> = Cow<'a, [u8]>;
type KeyPrefix<'a> = Cow<'a, [u8]>;

macro_rules! query_struct {
    ($name: ident<$lt: lifetime> {$($member: ident: $type: ty),*}) => {
        #[derive(Debug, BorshSerialize, BorshDeserialize)]
        pub struct $name<$lt> {
            pub table: TableName<$lt>,
            $(pub $member: $type,)*
        }
    };
}

// TODO: ScanKeys, ScanKeysByKeyPrefix, BatchScanKeys,...

query_struct!(Put<'a>{
    key: Key<'a>,
    value: Value<'a>,
    is_atomic: u8
});

query_struct!(Get<'a>{
    key: Key<'a>
});

query_struct!(Delete<'a>{
    key: Key<'a>,
    is_atomic: u8
});

query_struct!(DeleteByPrefix<'a>{
    key_prefix: KeyPrefix<'a>
});

query_struct!(Scan<'a>{
    key_prefix: KeyPrefix<'a>,
    limit: u32
});

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
    pub table_key_prefixe_tuples: Vec<(TableName<'a>, KeyPrefix<'a>)>,
    pub each_limit: u32,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableList<'a> {
    pub table_prefix: Cow<'a, [u8]>,
}
