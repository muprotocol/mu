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
    fn request(&mut self, req: OM) -> Result<IM<'static>> {
        self.context.write_message(req)?;
        self.context.read_message()
    }

    pub fn batch_put<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_value_triples: Vec<(&'b str, T, T)>,
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

    pub fn batch_get<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_tuples: Vec<(&'b str, T)>,
    ) -> Result<Vec<(TableName, Key, Value)>> {
        let req = BatchGet {
            table_key_tuples: vec_tuple_cow_u8_from(table_key_tuples),
        };
        let resp = self.request(OM::BatchGet(req))?;
        resp_to_triples(resp, "BatchGet")
    }

    pub fn batch_delete<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_tuples: Vec<(&'b str, T)>,
    ) -> Result<()> {
        let req = BatchDelete {
            table_key_tuples: vec_tuple_cow_u8_from(table_key_tuples),
        };
        let resp = self.request(OM::BatchDelete(req))?;
        resp_to_tuple_type(resp, "BatchDelete")
    }

    pub fn batch_scan<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_prefix_tuples: Vec<(&'b str, T)>,
        each_limit: u32,
    ) -> Result<Vec<(TableName, Key, Value)>> {
        let req = BatchScan {
            table_key_prefix_tuples: vec_tuple_cow_u8_from(table_key_prefix_tuples),
            each_limit,
        };
        let resp = self.request(OM::BatchScan(req))?;
        resp_to_triples(resp, "BatchScan")
    }

    pub fn batch_scan_keys<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_prefix_tuples: Vec<(&'b str, T)>,
        each_limit: u32,
    ) -> Result<Vec<Key>> {
        let req = BatchScanKeys {
            table_key_prefix_tuples: vec_tuple_cow_u8_from(table_key_prefix_tuples),
            each_limit,
        };
        let resp = self.request(OM::BatchScanKeys(req))?;
        resp_to_blobs(resp, "BatchScan")
    }

    pub fn table_list(&mut self, table_prefix: &str) -> Result<Vec<TableName>> {
        let req = TableList {
            table_prefix: Cow::Borrowed(table_prefix.as_bytes()),
        };

        let resp = self.request(OM::TableList(req))?;
        resp_to_blobs(resp, "TableList")?
            .into_iter()
            .map(String::from_utf8)
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| Error::DatabaseError(e.to_string()))
    }

    // per table requests

    pub fn put<'b, T: Into<&'b [u8]>>(
        &mut self,
        table: &str,
        key: T,
        value: T,
        is_atomic: bool,
    ) -> Result<()> {
        let req = Put {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.into()),
            value: Cow::Borrowed(value.into()),
            is_atomic,
        };
        let resp = self.request(OM::Put(req))?;
        resp_to_tuple_type(resp, "Put")
    }

    pub fn get<'b>(&mut self, table: &str, key: impl Into<&'b [u8]>) -> Result<Option<Value>> {
        let req = Get {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.into()),
        };
        let resp = self.request(OM::Get(req))?;
        resp_to_option_blob(resp, "Get")
    }

    pub fn delete<'b>(
        &mut self,
        table: &str,
        key: impl Into<&'b [u8]>,
        is_atomic: bool,
    ) -> Result<()> {
        let req = Delete {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.into()),
            is_atomic,
        };
        let resp = self.request(OM::Delete(req))?;
        resp_to_tuple_type(resp, "Delete")
    }

    pub fn delete_by_prefix<'b>(
        &mut self,
        table: &str,
        key_prefix: impl Into<&'b [u8]>,
    ) -> Result<()> {
        let req = DeleteByPrefix {
            table: Cow::Borrowed(table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.into()),
        };
        let resp = self.request(OM::DeleteByPrefix(req))?;
        resp_to_tuple_type(resp, "DeleteByPrefix")
    }

    pub fn scan<'b>(
        &mut self,
        table: &str,
        key_prefix: impl Into<&'b [u8]>,
        limit: u32,
    ) -> Result<Vec<(Key, Value)>> {
        let req = Scan {
            table: Cow::Borrowed(table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.into()),
            limit,
        };
        let resp = self.request(OM::Scan(req))?;
        resp_to_pairs(resp, "Scan")
    }

    pub fn scan_keys<'b>(
        &mut self,
        table: &str,
        key_prefix: impl Into<&'b [u8]>,
        limit: u32,
    ) -> Result<Vec<Key>> {
        let req = ScanKeys {
            table: Cow::Borrowed(table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.into()),
            limit,
        };
        let resp = self.request(OM::ScanKeys(req))?;
        resp_to_blobs(resp, "ScanKeys")
    }

    pub fn compare_and_swap<'b, T: Into<&'b [u8]>>(
        &mut self,
        table: &str,
        key: T,
        previous_value: Option<T>,
        new_value: T,
    ) -> Result<(Option<Value>, bool)> {
        let req = CompareAndSwap {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.into()),
            new_value: Cow::Borrowed(new_value.into()),
            previous_value: previous_value.map(Into::into).map(Cow::Borrowed).into(),
        };
        let resp = self.request(OM::CompareAndSwap(req))?;
        match resp {
            IM::CasResult(x) => Ok((
                Option::<Cow<[u8]>>::from(x.previous_value).map(|x| x.into_owned()),
                x.is_swapped,
            )),
            left => resp_to_err(left, "CompareAndSwap"),
        }
    }
}

fn resp_to_tuple_type(resp: IM, kind_name: &'static str) -> Result<()> {
    match resp {
        IM::EmptyResult(_) => Ok(()),
        left => resp_to_err(left, kind_name),
    }
}

fn resp_to_option_blob(resp: IM, kind_name: &'static str) -> Result<Option<Blob>> {
    match resp {
        IM::SingleResult(x) => Ok(Some(x.item.into_owned())),
        IM::EmptyResult(_) => Ok(None),
        left => resp_to_err(left, kind_name),
    }
}

fn resp_to_blobs(resp: IM, kind_name: &'static str) -> Result<Vec<Blob>> {
    match resp {
        IM::ListResult(x) => Ok(x.items.into_iter().map(Into::into).collect()),
        left => resp_to_err(left, kind_name),
    }
}

fn resp_to_pairs(resp: IM, kind_name: &'static str) -> Result<Vec<(Blob, Blob)>> {
    match resp {
        IM::KvPairsResult(x) => Ok(x
            .kv_pairs
            .into_iter()
            .map(|pair| (pair.key.into(), pair.value.into()))
            .collect()),
        left => resp_to_err(left, kind_name),
    }
}

fn resp_to_triples(resp: IM, kind_name: &'static str) -> Result<Vec<(String, Blob, Blob)>> {
    match resp {
        IM::TkvTriplesResult(x) => Ok(x
            .tkv_triples
            .into_iter()
            .map(|triple| (triple.table.into(), triple.key.into(), triple.value.into()))
            .collect()),
        left => resp_to_err(left, kind_name),
    }
}

fn resp_to_err<T>(left: IM, kind_name: &'static str) -> Result<T> {
    match left {
        IM::DbError(e) => Err(Error::DatabaseError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

fn vec_tuple_cow_u8_from<'a, T>(pairs: Vec<(&'a str, T)>) -> Vec<(Cow<[u8]>, Cow<[u8]>)>
where
    T: Into<&'a [u8]>,
{
    let into_tuple_cow_u8 =
        |y: (&'a str, T)| (Cow::Borrowed(y.0.as_bytes()), Cow::Borrowed(y.1.into()));

    pairs.into_iter().map(into_tuple_cow_u8).collect()
}
