use std::borrow::Cow;

use mu_db::{error::Result, Key, Scan};
use mu_stack::StackID;
use musdk_common::incoming_message::{
    db::{
        CasResult, EmptyResult, KeyValue, KeyValueListResult, ListResult, SingleResult, TableKey,
        TableKeyListResult, TableKeyValue, TableKeyValueListResult,
    },
    IncomingMessage,
};

pub fn make_mudb_key(
    stack_id: StackID,
    cow_table: Cow<'_, [u8]>,
    cow_key: Cow<'_, [u8]>,
) -> Result<Key> {
    Ok(Key {
        stack_id,
        table_name: cow_table.into_owned().try_into()?,
        inner_key: cow_key.into_owned(),
    })
}

pub fn make_mudb_scan(
    stack_id: StackID,
    cow_table: Cow<'_, [u8]>,
    cow_key_prefix: Cow<'_, [u8]>,
) -> Result<Scan> {
    Ok(Scan::ByInnerKeyPrefix(
        stack_id,
        cow_table.into_owned().try_into()?,
        cow_key_prefix.into_owned(),
    ))
}

pub type TableKeyPairs<'a> = Vec<(Cow<'a, [u8]>, Cow<'a, [u8]>)>;

pub fn make_mudb_keys(stack_id: StackID, table_key_list: TableKeyPairs) -> Result<Vec<Key>> {
    table_key_list
        .into_iter()
        .map(|(table, key)| make_mudb_key(stack_id, table, key))
        .collect::<Result<_>>()
}

pub fn make_mudb_scans(stack_id: StackID, table_key_list: TableKeyPairs) -> Result<Vec<Scan>> {
    table_key_list
        .into_iter()
        .map(|(table, key)| make_mudb_scan(stack_id, table, key))
        .collect::<Result<_>>()
}

pub fn into_single_or_empty_incoming_msg<'a>(x: Option<Vec<u8>>) -> IncomingMessage<'a> {
    match x {
        Some(xp) => IncomingMessage::SingleResult(SingleResult {
            item: Cow::Owned(xp),
        }),
        None => IncomingMessage::EmptyResult(EmptyResult),
    }
}

pub fn into_empty_incoming_msg<'a>(_: ()) -> IncomingMessage<'a> {
    IncomingMessage::EmptyResult(EmptyResult)
}

pub fn into_kv_pairs_incoming_msg<'a>(x: Vec<(Key, Vec<u8>)>) -> IncomingMessage<'a> {
    IncomingMessage::KeyValueListResult(KeyValueListResult {
        list: x
            .into_iter()
            .map(|(k, v)| KeyValue {
                key: Cow::Owned(k.inner_key),
                value: Cow::Owned(v),
            })
            .collect(),
    })
}

pub fn into_tk_pairs_incoming_msg<'a>(x: Vec<Key>) -> IncomingMessage<'a> {
    IncomingMessage::TableKeyListResult(TableKeyListResult {
        list: x
            .into_iter()
            .map(|k| TableKey {
                table: Cow::Owned(k.table_name.into()),
                key: Cow::Owned(k.inner_key),
            })
            .collect(),
    })
}

pub fn into_tkv_triples_incoming_msg<'a>(x: Vec<(Key, Vec<u8>)>) -> IncomingMessage<'a> {
    IncomingMessage::TableKeyValueListResult(TableKeyValueListResult {
        list: x
            .into_iter()
            .map(|(k, v)| TableKeyValue {
                table: Cow::Owned(k.table_name.into()),
                key: Cow::Owned(k.inner_key),
                value: Cow::Owned(v),
            })
            .collect(),
    })
}

pub fn into_list_incoming_msg<'a, I, T>(x: I) -> IncomingMessage<'a>
where
    I: IntoIterator<Item = T>,
    Vec<u8>: From<T>,
{
    IncomingMessage::ListResult(ListResult {
        list: x.into_iter().map(Vec::<u8>::from).map(Cow::Owned).collect(),
    })
}

pub fn into_cas_incoming_msg<'a>(x: (Option<Vec<u8>>, bool)) -> IncomingMessage<'a> {
    IncomingMessage::CasResult(CasResult {
        previous_value: x.0.map(Cow::Owned),
        is_swapped: x.1,
    })
}
