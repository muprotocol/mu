use std::borrow::Cow;

use musdk_common::{
    incoming_message::IncomingMessage as IM,
    outgoing_message::{db::*, OutgoingMessage as OM},
};

use crate::{Error, Result};

type Blob = Vec<u8>;
// TODO: make these strong type
type Key = Vec<u8>;
type Value = Vec<u8>;
type TableName = String;

// TODO
// struct Key<'a>(Cow<'a, [u8]>);
//
// impl<'a> Deref for Key<'a> {
//     type Target = Cow<'a, [u8]>;
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }
//
// impl<'a, T> From<T> for Key<'a>
// where
//     T: Into<&'a [u8]>,
// {
//     fn from(value: T) -> Self {
//         Self(Cow::Borrowed(value.into()))
//     }
// }

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
            is_atomic,
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
                .map(into_tuple_cow_u8)
                .collect(),
        };
        let resp = self.request(OM::BatchGet(req))?;
        resp_to_vec_tuple_blob(resp, "BatchGet")
    }

    pub fn batch_delete<T: Into<&'a [u8]>>(
        &mut self,
        table_key_tuples: Vec<(&'a str, T)>,
    ) -> Result<()> {
        let req = BatchDelete {
            table_key_tuples: table_key_tuples
                .into_iter()
                .map(into_tuple_cow_u8)
                .collect(),
        };
        let resp = self.request(OM::BatchDelete(req))?;
        resp_to_tuple_type(resp, "BatchDelete")
    }

    pub fn batch_scan<T: Into<&'a [u8]>>(
        &mut self,
        table_key_prefixe_tuples: Vec<(&'a str, T)>,
        each_limit: u32,
    ) -> Result<Vec<(Key, Value)>> {
        let req = BatchScan {
            table_key_prefixe_tuples: table_key_prefixe_tuples
                .into_iter()
                .map(into_tuple_cow_u8)
                .collect(),
            each_limit,
        };
        let resp = self.request(OM::BatchScan(req))?;
        resp_to_vec_tuple_blob(resp, "BatchScan")
    }

    pub fn batch_scan_keys<T: Into<&'a [u8]>>(
        &mut self,
        table_key_prefixe_tuples: Vec<(&'a str, T)>,
        each_limit: u32,
    ) -> Result<Vec<Key>> {
        let req = BatchScanKeys {
            table_key_prefixe_tuples: table_key_prefixe_tuples
                .into_iter()
                .map(into_tuple_cow_u8)
                .collect(),
            each_limit,
        };
        let resp = self.request(OM::BatchScanKeys(req))?;
        resp_to_vec_blob(resp, "BatchScan")
    }

    pub fn table_list(&mut self, table_prefix: &'a str) -> Result<Vec<TableName>> {
        let req = TableList {
            table_prefix: Cow::Borrowed(table_prefix.as_bytes()),
        };
        let resp = self.request(OM::TableList(req))?;
        resp_to_vec_blob(resp, "TableList")
            .map(Vec::into_iter)?
            .map(String::from_utf8)
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| Error::DatabaseError(e.to_string()))
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
            is_atomic,
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
        resp_to_option_blob(resp, "Get")
    }

    pub fn delete(&mut self, key: impl Into<&'a [u8]>, is_atomic: bool) -> Result<()> {
        let req = Delete {
            table: Cow::Borrowed(self.table.as_bytes()),
            key: Cow::Borrowed(key.into()),
            is_atomic,
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
        resp_to_vec_tuple_blob(resp, "Scan")
    }

    pub fn scan_keys(&mut self, key_prefix: impl Into<&'a [u8]>, limit: u32) -> Result<Vec<Key>> {
        let req = ScanKeys {
            table: Cow::Borrowed(self.table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.into()),
            limit,
        };
        let resp = self.db.request(OM::ScanKeys(req))?;
        resp_to_vec_blob(resp, "ScanKeys")
    }

    pub fn compare_and_swap<T: Into<&'a [u8]>>(
        &mut self,
        key: T,
        new_value: T,
        previous_value: Option<T>,
    ) -> Result<(Option<Value>, bool)> {
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

        match resp {
            IM::CasResult(x) => Ok(if x.is_swapped {
                (Some(x.previous_value.into_owned()), true)
            } else {
                (None, false)
            }),
            tail => resp_tail_to_err(tail, "CompareAndSwap"),
        }
    }
}

fn resp_to_tuple_type(resp: IM, kind_name: &'static str) -> Result<()> {
    match resp {
        IM::EmptyResult(_) => Ok(()),
        tail => resp_tail_to_err(tail, kind_name),
    }
}

fn resp_to_option_blob(resp: IM, kind_name: &'static str) -> Result<Option<Blob>> {
    match resp {
        IM::SingleResult(x) => Ok(Some(x.item.into_owned())),
        IM::EmptyResult(_) => Ok(None),
        tail => resp_tail_to_err(tail, kind_name),
    }
}

fn resp_to_vec_blob(resp: IM, kind_name: &'static str) -> Result<Vec<Blob>> {
    match resp {
        IM::ListResult(x) => Ok(x.items.into_iter().map(Into::into).collect()),
        tail => resp_tail_to_err(tail, kind_name),
    }
}

fn resp_to_vec_tuple_blob(resp: IM, kind_name: &'static str) -> Result<Vec<(Blob, Blob)>> {
    match resp {
        IM::KvPairsResult(x) => Ok(x
            .kv_pairs
            .into_iter()
            .map(|pair| (pair.key.into(), pair.value.into()))
            .collect()),
        tail => resp_tail_to_err(tail, kind_name),
    }
}

fn resp_tail_to_err<T>(tail: IM, kind_name: &'static str) -> Result<T> {
    match tail {
        IM::DbError(e) => Err(Error::DatabaseError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

fn into_tuple_cow_u8<'a>(x: (&'a str, impl Into<&'a [u8]>)) -> (Cow<'_, [u8]>, Cow<'_, [u8]>) {
    (Cow::Borrowed(x.0.as_bytes()), Cow::Borrowed(x.1.into()))
}
