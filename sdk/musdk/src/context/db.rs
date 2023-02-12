use std::{borrow::Cow, ops::Deref};

use musdk_common::{
    incoming_message::IncomingMessage as IM,
    outgoing_message::{db::*, OutgoingMessage as OM},
};

use crate::{Error, Result};

type Blob = Vec<u8>;

pub struct TableName(pub String);

impl Deref for TableName {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> From<T> for TableName
where
    T: Into<String>,
{
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

pub struct Key(pub Blob);

impl Deref for Key {
    type Target = Blob;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> From<T> for Key
where
    T: Into<Blob>,
{
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

pub struct Value(pub Blob);

impl Deref for Value {
    type Target = Blob;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> From<T> for Value
where
    T: Into<Blob>,
{
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

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
        from_empty_resp(resp, "BatchPut")
    }

    pub fn batch_get<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_tuples: Vec<(&'b str, T)>,
    ) -> Result<Vec<(TableName, Key, Value)>> {
        let req = BatchGet {
            table_key_tuples: vec_tuple_cow_u8_from(table_key_tuples),
        };
        let resp = self.request(OM::BatchGet(req))?;
        from_table_key_value_list_resp(resp, "BatchGet")
    }

    pub fn batch_delete<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_tuples: Vec<(&'b str, T)>,
    ) -> Result<()> {
        let req = BatchDelete {
            table_key_tuples: vec_tuple_cow_u8_from(table_key_tuples),
        };
        let resp = self.request(OM::BatchDelete(req))?;
        from_empty_resp(resp, "BatchDelete")
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
        from_table_key_value_list_resp(resp, "BatchScan")
    }

    pub fn batch_scan_keys<'b, T: Into<&'b [u8]>>(
        &mut self,
        table_key_prefix_tuples: Vec<(&'b str, T)>,
        each_limit: u32,
    ) -> Result<Vec<(TableName, Key)>> {
        let req = BatchScanKeys {
            table_key_prefix_tuples: vec_tuple_cow_u8_from(table_key_prefix_tuples),
            each_limit,
        };
        let resp = self.request(OM::BatchScanKeys(req))?;
        from_table_key_list_resp(resp, "BatchScan")
    }

    pub fn table_list(&mut self, table_prefix: &str) -> Result<Vec<TableName>> {
        let req = TableList {
            table_prefix: Cow::Borrowed(table_prefix.as_bytes()),
        };

        let resp = self.request(OM::TableList(req))?;
        from_list_resp(resp, "TableList")?
            .map(Vec::from)
            .map(String::from_utf8)
            .map(|x| x.map(TableName::from))
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
        from_empty_resp(resp, "Put")
    }

    pub fn get<'b>(&mut self, table: &str, key: impl Into<&'b [u8]>) -> Result<Option<Value>> {
        let req = Get {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.into()),
        };
        let resp = self.request(OM::Get(req))?;
        from_maybe_single_or_empty_resp(resp, "Get")
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
        from_empty_resp(resp, "Delete")
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
        from_empty_resp(resp, "DeleteByPrefix")
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
        from_kv_pairs_resp(resp, "Scan")
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
        Ok(from_list_resp(resp, "ScanKeys")?.map(Key::from).collect())
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
            previous_value: previous_value.map(Into::into).map(Cow::Borrowed),
        };
        let resp = self.request(OM::CompareAndSwap(req))?;
        match resp {
            IM::CasResult(x) => Ok((x.previous_value.map(Value::from), x.is_swapped)),
            left => resp_to_err(left, "CompareAndSwap"),
        }
    }
}

fn from_empty_resp(resp: IM, kind_name: &'static str) -> Result<()> {
    match resp {
        IM::EmptyResult(_) => Ok(()),
        left => resp_to_err(left, kind_name),
    }
}

fn from_maybe_single_or_empty_resp(resp: IM, kind_name: &'static str) -> Result<Option<Value>> {
    match resp {
        IM::SingleResult(x) => Ok(Some(x.item.into())),
        IM::EmptyResult(_) => Ok(None),
        left => resp_to_err(left, kind_name),
    }
}

fn from_list_resp<'a>(
    resp: IM<'a>,
    kind_name: &'static str,
) -> Result<impl Iterator<Item = Cow<'a, [u8]>>> {
    match resp {
        IM::ListResult(x) => Ok(x.list.into_iter()),
        left => resp_to_err(left, kind_name),
    }
}

fn from_kv_pairs_resp(resp: IM, kind_name: &'static str) -> Result<Vec<(Key, Value)>> {
    match resp {
        IM::KeyValueListResult(x) => Ok(x
            .list
            .into_iter()
            .map(|pair| (pair.key.into(), pair.value.into()))
            .collect()),
        left => resp_to_err(left, kind_name),
    }
}

fn from_table_key_list_resp(resp: IM, kind_name: &'static str) -> Result<Vec<(TableName, Key)>> {
    match resp {
        IM::TableKeyListResult(x) => Ok(x
            .list
            .into_iter()
            .map(|pair| (pair.table.into(), pair.key.into()))
            .collect()),
        left => resp_to_err(left, kind_name),
    }
}

fn from_table_key_value_list_resp(
    resp: IM,
    kind_name: &'static str,
) -> Result<Vec<(TableName, Key, Value)>> {
    match resp {
        IM::TableKeyValueListResult(x) => Ok(x
            .list
            .into_iter()
            .map(|triple| (triple.table.into(), triple.key.into(), triple.value.into()))
            .collect()),
        left => resp_to_err(left, kind_name),
    }
}

fn resp_to_err<T>(resp: IM, kind_name: &'static str) -> Result<T> {
    match resp {
        IM::DbError(e) => Err(Error::DatabaseError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

#[allow(clippy::type_complexity)]
fn vec_tuple_cow_u8_from<'a, T>(pairs: Vec<(&'a str, T)>) -> Vec<(Cow<[u8]>, Cow<[u8]>)>
where
    T: Into<&'a [u8]>,
{
    let into_tuple_cow_u8 =
        |y: (&'a str, T)| (Cow::Borrowed(y.0.as_bytes()), Cow::Borrowed(y.1.into()));

    pairs.into_iter().map(into_tuple_cow_u8).collect()
}
