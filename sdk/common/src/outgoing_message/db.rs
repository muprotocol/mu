use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

macro_rules! query_struct {
    ($name: ident<$lt: lifetime> {$($member: ident: $type: ty),*}) => {
        #[derive(Debug, BorshSerialize, BorshDeserialize)]
        pub struct $name<$lt> {
            pub table: Cow<$lt, [u8]>,
            $(pub $member: $type,)*
        }
    };
}

// TODO: ScanKeys, ScanKeysByKeyPrefix, BatchScanKeys,...

query_struct!(Put<'a>{
    key: Cow<'a, [u8]>,
    value: Cow<'a, [u8]>,
    is_atomic: u8
});

query_struct!(Get<'a>{
    key: Cow<'a, [u8]>
});

query_struct!(Delete<'a>{
    key: Cow<'a, [u8]>,
    is_atomic: u8
});

query_struct!(DeleteByPrefix<'a>{
    key_prefix: Cow<'a, [u8]>
});

query_struct!(Scan<'a>{
    key_prefix: Cow<'a, [u8]>,
    limit: u32
});

query_struct!(ScanKeys<'a>{
    key_prefix: Cow<'a, [u8]>,
    limit: u32
});

query_struct!(CompareAndSwap<'a>{
    key: Cow<'a, [u8]>,
    new_value: Cow<'a, [u8]>,
    is_previous_value_exist: u8,
    previous_value: Cow<'a, [u8]>
});

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchPut<'a> {
    pub table_key_value_triples: Vec<(Cow<'a, [u8]>, Cow<'a, [u8]>, Cow<'a, [u8]>)>,
    pub is_atomic: bool,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchGet<'a> {
    pub table_key_tuples: Vec<(Cow<'a, [u8]>, Cow<'a, [u8]>)>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchDelete<'a> {
    pub table_key_tuples: Vec<(Cow<'a, [u8]>, Cow<'a, [u8]>)>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchScan<'a> {
    pub table_key_prefixe_tuples: Vec<(Cow<'a, [u8]>, Cow<'a, [u8]>)>,
    pub each_limit: u32,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct BatchScanKeys<'a> {
    pub table_key_prefixe_tuples: Vec<(Cow<'a, [u8]>, Cow<'a, [u8]>)>,
    pub each_limit: u32,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableList<'a> {
    pub table_prefix: Cow<'a, [u8]>,
}
