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

    pub fn batch_put<'b, K: AsRef<[u8]> + 'b, V: AsRef<[u8]> + 'b>(
        &mut self,
        table_key_value_triples: impl IntoIterator<Item = &'b (&'b str, K, V)>,
        is_atomic: bool,
    ) -> Result<()> {
        let req = BatchPut {
            table_key_value_triples: table_key_value_triples
                .into_iter()
                .map(|(t, k, v)| {
                    (
                        Cow::Borrowed(t.as_bytes()),
                        Cow::Borrowed(k.as_ref()),
                        Cow::Borrowed(v.as_ref()),
                    )
                })
                .collect(),
            is_atomic,
        };
        let resp = self.request(OM::BatchPut(req))?;
        from_empty_resp(resp, "BatchPut")
    }

    pub fn batch_get<'b, T: AsRef<[u8]> + 'b>(
        &mut self,
        table_key_tuples: impl IntoIterator<Item = &'b (&'b str, T)>,
    ) -> Result<Vec<(TableName, Key, Value)>> {
        let req = BatchGet {
            table_key_tuples: vec_tuple_cow_u8_from(table_key_tuples),
        };
        let resp = self.request(OM::BatchGet(req))?;
        from_table_key_value_list_resp(resp, "BatchGet")
    }

    pub fn batch_delete<'b, T: AsRef<[u8]> + 'b>(
        &mut self,
        table_key_tuples: impl IntoIterator<Item = &'b (&'b str, T)>,
    ) -> Result<()> {
        let req = BatchDelete {
            table_key_tuples: vec_tuple_cow_u8_from(table_key_tuples),
        };
        let resp = self.request(OM::BatchDelete(req))?;
        from_empty_resp(resp, "BatchDelete")
    }

    pub fn batch_scan<'b, T: AsRef<[u8]> + 'b>(
        &mut self,
        table_key_prefix_tuples: impl IntoIterator<Item = &'b (&'b str, T)>,
        each_limit: u32,
    ) -> Result<Vec<(TableName, Key, Value)>> {
        let req = BatchScan {
            table_key_prefix_tuples: vec_tuple_cow_u8_from(table_key_prefix_tuples),
            each_limit,
        };
        let resp = self.request(OM::BatchScan(req))?;
        from_table_key_value_list_resp(resp, "BatchScan")
    }

    pub fn batch_scan_keys<'b, T: AsRef<[u8]> + 'b>(
        &mut self,
        table_key_prefix_tuples: impl IntoIterator<Item = &'b (&'b str, T)>,
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

    pub fn put<K: AsRef<[u8]>, V: AsRef<[u8]>>(
        &mut self,
        table: &str,
        key: K,
        value: V,
        is_atomic: bool,
    ) -> Result<()> {
        let req = Put {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.as_ref()),
            value: Cow::Borrowed(value.as_ref()),
            is_atomic,
        };
        let resp = self.request(OM::Put(req))?;
        from_empty_resp(resp, "Put")
    }

    pub fn get(&mut self, table: &str, key: impl AsRef<[u8]>) -> Result<Option<Value>> {
        let req = Get {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.as_ref()),
        };
        let resp = self.request(OM::Get(req))?;
        from_maybe_single_or_empty_resp(resp, "Get")
    }

    pub fn delete(&mut self, table: &str, key: impl AsRef<[u8]>, is_atomic: bool) -> Result<()> {
        let req = Delete {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.as_ref()),
            is_atomic,
        };
        let resp = self.request(OM::Delete(req))?;
        from_empty_resp(resp, "Delete")
    }

    pub fn delete_by_prefix(&mut self, table: &str, key_prefix: impl AsRef<[u8]>) -> Result<()> {
        let req = DeleteByPrefix {
            table: Cow::Borrowed(table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.as_ref()),
        };
        let resp = self.request(OM::DeleteByPrefix(req))?;
        from_empty_resp(resp, "DeleteByPrefix")
    }

    pub fn scan(
        &mut self,
        table: &str,
        key_prefix: impl AsRef<[u8]>,
        limit: u32,
    ) -> Result<Vec<(Key, Value)>> {
        let req = Scan {
            table: Cow::Borrowed(table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.as_ref()),
            limit,
        };
        let resp = self.request(OM::Scan(req))?;
        from_kv_pairs_resp(resp, "Scan")
    }

    pub fn scan_keys(
        &mut self,
        table: &str,
        key_prefix: impl AsRef<[u8]>,
        limit: u32,
    ) -> Result<Vec<Key>> {
        let req = ScanKeys {
            table: Cow::Borrowed(table.as_bytes()),
            key_prefix: Cow::Borrowed(key_prefix.as_ref()),
            limit,
        };
        let resp = self.request(OM::ScanKeys(req))?;
        Ok(from_list_resp(resp, "ScanKeys")?.map(Key::from).collect())
    }

    pub fn compare_and_swap<K: AsRef<[u8]>, V: AsRef<[u8]>, PV: AsRef<[u8]>>(
        &mut self,
        table: &str,
        key: K,
        previous_value: Option<PV>,
        new_value: V,
    ) -> Result<(Option<Value>, bool)> {
        let req = CompareAndSwap {
            table: Cow::Borrowed(table.as_bytes()),
            key: Cow::Borrowed(key.as_ref()),
            new_value: Cow::Borrowed(new_value.as_ref()),
            previous_value: previous_value.as_ref().map(|pv| Cow::Borrowed(pv.as_ref())),
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
fn vec_tuple_cow_u8_from<'b, T: AsRef<[u8]> + 'b>(
    pairs: impl IntoIterator<Item = &'b (&'b str, T)>,
) -> Vec<(Cow<'b, [u8]>, Cow<'b, [u8]>)>
where
    T: AsRef<[u8]>,
{
    pairs
        .into_iter()
        .map(|(t, k)| (t.as_bytes().into(), k.as_ref().into()))
        .collect()
}
