use std::borrow::Cow;

use musdk_common::{
    incoming_message::IncomingMessage as IM,
    outgoing_message::{db::*, OutgoingMessage as OM},
};

use crate::{Error, Result};

type Key = Vec<u8>;
type Value = Vec<u8>;

pub struct DbHandle<'a> {
    pub(super) context: &'a mut super::MuContext,
}

impl<'a> DbHandle<'a> {
    pub fn table<'b: 'a>(&'b mut self, table: &'b str) -> TableHandle {
        TableHandle { db: self, table }
    }

    fn request(&mut self, req: OM) -> Result<IM<'a>> {
        self.context.write_message(req)?;
        self.context.read_message()
    }

    pub fn batch_put<T: Into<&'a [u8]>>(
        &mut self,
        table_key_value_triples: Vec<(&'a str, T, T)>,
        is_atomic: bool,
    ) -> Result<()> {
        let req = BatchPut {
            table_key_value_triples: table_key_value_triples
                .into_iter()
                .map(|(t, k, v)| {
                    (
                        Cow::Borrowed(t.as_bytes()),
                        Cow::Borrowed(k.into()),
                        Cow::Borrowed(v.into()),
                    )
                })
                .collect(),
            is_atomic: is_atomic.into(),
        };
        let resp = self.request(OM::BatchPut(req))?;
        resp_to_tuple_type(resp, "BatchPut")
    }

    pub fn batch_get<T: Into<&'a [u8]>>(
        &mut self,
        table_key_tuples: Vec<(&'a str, T)>,
    ) -> Result<Vec<(Key, Value)>> {
        let req = BatchGet {
            table_key_tuples: table_key_tuples
                .into_iter()
                .map(make_cow_table_key_pair)
                .collect(),
        };
        let resp = self.request(OM::BatchGet(req))?;
        resp_to_kv_pairs(resp, "BatchGet")
    }

    pub fn batch_delete<T: Into<&'a [u8]>>(
        &mut self,
        table_key_tuples: Vec<(&'a str, T)>,
    ) -> Result<Vec<(Key, Value)>> {
        let req = BatchDelete {
            table_key_tuples: table_key_tuples
                .into_iter()
                .map(make_cow_table_key_pair)
                .collect(),
        };
        let resp = self.request(OM::BatchDelete(req))?;
        resp_to_kv_pairs(resp, "BatchDelete")
    }

    pub fn batch_scan<T: Into<&'a [u8]>>(
        &mut self,
        table_key_prefixe_tuples: Vec<(&'a str, T)>,
        each_limit: u32,
    ) -> Result<Vec<(Key, Value)>> {
        let req = BatchScan {
            table_key_prefixe_tuples: table_key_prefixe_tuples
                .into_iter()
                .map(make_cow_table_key_pair)
                .collect(),
            each_limit,
        };
        let resp = self.request(OM::BatchScan(req))?;
        resp_to_kv_pairs(resp, "BatchScan")
    }

    pub fn batch_scan_keys<T: Into<&'a [u8]>>(
        &mut self,
        table_key_prefixe_tuples: Vec<(&'a str, T)>,
        each_limit: u32,
    ) -> Result<Vec<Key>> {
        let req = BatchScanKeys {
            table_key_prefixe_tuples: table_key_prefixe_tuples
                .into_iter()
                .map(make_cow_table_key_pair)
                .collect(),
            each_limit,
        };
        let resp = self.request(OM::BatchScanKeys(req))?;
        resp_to_keys(resp, "BatchScan")
    }

    pub fn table_list(&mut self, table_prefix: &'a str) -> Result<Vec<(Key, Value)>> {
        let req = TableList {
            table_prefix: Cow::Borrowed(table_prefix.as_bytes()),
        };
        let resp = self.request(OM::TableList(req))?;
        resp_to_kv_pairs(resp, "TableList")
    }
}

pub struct TableHandle<'a> {
    db: &'a mut DbHandle<'a>,
    table: &'a str,
}

impl<'a> TableHandle<'a> {
    pub fn put<T: Into<&'a [u8]>>(&mut self, key: T, value: T, is_atomic: bool) -> Result<()> {
        let req = Put {
            table: Cow::Borrowed(self.table.as_bytes()),
            key: Cow::Borrowed(key.into()),
            value: Cow::Borrowed(value.into()),
            is_atomic: is_atomic.into(),
        };
        let resp = self.db.request(OM::Put(req))?;
        resp_to_tuple_type(resp, "Put")
    }

    pub fn get(&mut self, key: impl Into<&'a [u8]>) -> Result<Option<Value>> {
        let req = Get {
            table: Cow::Borrowed(self.table.as_bytes()),
            key: Cow::Borrowed(key.into()),
        };
        let resp = self.db.request(OM::Get(req))?;
        resp_to_option_value(resp, "Get")
    }

    pub fn delete(&mut self, key: impl Into<&'a [u8]>, is_atomic: bool) -> Result<()> {
        let req = Delete {
            table: Cow::Borrowed(self.table.as_bytes()),
            key: Cow::Borrowed(key.into()),
            is_atomic: is_atomic.into(),
        };
        let resp = self.db.request(OM::Delete(req))?;
        resp_to_tuple_type(resp, "Delete")
    }

    pub fn delete_by_prefix(&mut self, key_prefix: impl Into<&'a [u8]>) -> Result<()> {
        let req = DeleteByPrefix {
            table: Cow::Borrowed(self.table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.into()),
        };
        let resp = self.db.request(OM::DeleteByPrefix(req))?;
        resp_to_tuple_type(resp, "DeleteByPrefix")
    }

    pub fn scan(
        &mut self,
        key_prefix: impl Into<&'a [u8]>,
        limit: u32,
    ) -> Result<Vec<(Key, Value)>> {
        let req = Scan {
            table: Cow::Borrowed(self.table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.into()),
            limit,
        };
        let resp = self.db.request(OM::Scan(req))?;
        resp_to_kv_pairs(resp, "Scan")
    }

    pub fn scan_keys(&mut self, key_prefix: impl Into<&'a [u8]>, limit: u32) -> Result<Vec<Key>> {
        let req = ScanKeys {
            table: Cow::Borrowed(self.table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.into()),
            limit,
        };
        let resp = self.db.request(OM::ScanKeys(req))?;
        resp_to_keys(resp, "ScanKeys")
    }

    pub fn compare_and_swap<T: Into<&'a [u8]>>(
        &mut self,
        key: T,
        new_value: T,
        previous_value: Option<T>,
    ) -> Result<Vec<(Key, Value)>> {
        let req = CompareAndSwap {
            table: Cow::Borrowed(self.table.as_bytes()),
            key: Cow::Borrowed(key.into()),
            new_value: Cow::Borrowed(new_value.into()),
            is_previous_value_exist: previous_value.is_some().into(),
            previous_value: Cow::Borrowed(match previous_value {
                Some(x) => x.into(),
                None => &[],
            }),
        };
        let resp = self.db.request(OM::CompareAndSwap(req))?;
        resp_to_kv_pairs(resp, "Scan")
    }
}

fn resp_to_tuple_type(resp: IM, kind_name: &'static str) -> Result<()> {
    match resp {
        IM::EmptyResult(_) => Ok(()),
        IM::DbError(e) => Err(Error::DatabaseError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

fn resp_to_option_value(resp: IM, kind_name: &'static str) -> Result<Option<Value>> {
    match resp {
        IM::SingleResult(x) => Ok(Some(x.key_or_value.into_owned())),
        IM::EmptyResult(_) => Ok(None),
        IM::DbError(e) => Err(Error::DatabaseError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

fn resp_to_keys(resp: IM, kind_name: &'static str) -> Result<Vec<Key>> {
    match resp {
        IM::KeyListResult(x) => Ok(x.keys.into_iter().map(Into::into).collect()),
        IM::DbError(e) => Err(Error::DatabaseError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

fn resp_to_kv_pairs(resp: IM, kind_name: &'static str) -> Result<Vec<(Key, Value)>> {
    match resp {
        IM::KvPairsResult(x) => Ok(x
            .kv_pairs
            .into_iter()
            .map(|pair| (pair.key.into(), pair.value.into()))
            .collect()),
        IM::DbError(e) => Err(Error::DatabaseError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

fn make_cow_table_key_pair<'a>(
    x: (&'a str, impl Into<&'a [u8]>),
) -> (Cow<'_, [u8]>, Cow<'_, [u8]>) {
    (Cow::Borrowed(x.0.as_bytes()), Cow::Borrowed(x.1.into()))
}
