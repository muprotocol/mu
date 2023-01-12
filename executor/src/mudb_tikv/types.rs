use super::error::Result;
use async_trait::async_trait;
use bytes::BufMut;
use mu_stack::StackID;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::ops::Deref;
use tikv_client::{BoundRange, Key as TikvKey, Value};

// TODO: add constraint to Key (actually key.stack_id) to avoid this name
// TODO: rename it
const TABLE_LIST: &str = "__tl";

pub type Blob = Vec<u8>;

fn tikv_key_from_3_chunk(first: &[u8], second: &[u8], third: &[u8]) -> TikvKey {
    let mut x: Blob = Vec::with_capacity(first.len() + second.len() + third.len() + 2);
    assert!(first.len() <= u8::MAX as usize);
    x.push(first.len() as u8);
    x.put_slice(first);
    assert!(second.len() <= u8::MAX as usize);
    x.push(second.len() as u8);
    x.put_slice(second);
    x.put_slice(third);
    x.into()
}

fn three_chunk_try_from_tikv_key(
    value: tikv_client::Key,
) -> std::result::Result<(Blob, Blob, Blob), String> {
    let e = "Insufficient blobs to convert to Key";
    let split_first = |mut x: Vec<u8>| {
        if x.len() < 1 {
            Err(e.to_string())
        } else {
            let y = x.split_off(x.len() - 1);
            let x = x.pop().unwrap();
            Ok((x, y))
        }
    };
    let split_at = |mut x: Vec<u8>, y| {
        if x.len() < y {
            Err(e.to_string())
        } else {
            let z = x.split_off(x.len() - y);
            Ok((x, z))
        }
    };

    let x: Blob = value.into();

    let (a_size, x) = split_first(x)?;
    let (a, x) = split_at(x, a_size as usize)?;
    let (b_size, x) = split_first(x)?;
    let (b, c) = split_at(x, b_size as usize)?;

    Ok((a.into(), b.into(), c.into()))

    // TODO: remove old
    // let (a_size, r) = x.split_first().ok_or_else(|| e.to_string())?;
    // let a_size = *a_size as usize;
    // if r.len() < a_size {
    //     return Err(e.into());
    // }
    // let (a, r) = r.split_at(a_size);
    // let (b_size, r) = r.split_first().ok_or_else(|| e.to_string())?;
    // let b_size = *b_size as usize;
    // if r.len() < b_size {
    //     return Err(e.into());
    // }
    // let (b, c) = r.split_at(b_size);

    // Ok((a.into(), b.into(), c.into()))
}

pub struct TableListKey {
    pub stack_id: StackID,
    pub table_name: TableName,
}

impl TableListKey {
    pub fn new(stack_id: StackID, table_name: TableName) -> Self {
        Self {
            stack_id,
            table_name,
        }
    }
}

impl From<TableListKey> for tikv_client::Key {
    fn from(k: TableListKey) -> Self {
        let first = TABLE_LIST.as_bytes();
        // TODO
        let second = k.stack_id.get_bytes();
        let third = k.table_name.as_bytes();
        tikv_key_from_3_chunk(first, second, third)
    }
}

impl TryFrom<tikv_client::Key> for TableListKey {
    type Error = String;
    fn try_from(value: tikv_client::Key) -> std::result::Result<Self, Self::Error> {
        let (a, b, c) = three_chunk_try_from_tikv_key(value)?;
        if TABLE_LIST
            != &String::from_utf8(a)
                .map_err(|e| format!("cant deserialize {TABLE_LIST} cause: {e}"))?
        {
            Err(format!(
                "cant deserialize TableListKey cause it's not TableListKey"
            ))
        } else {
            Ok(Self {
                stack_id: b
                    .try_into()
                    .map_err(|_| "cant deserialize stack_id".to_string())?,
                table_name: c
                    .try_into()
                    .map_err(|_| "cant deserialize table_name".to_string())?,
            })
        }
    }
}

fn prefixed_by_a_chunk_bound_range(mut chunk: Blob) -> BoundRange {
    chunk.insert(0, chunk.len() as u8);
    subset_range(chunk)
}
fn prefixed_by_two_chunk_bound_range(mut first: Blob, mut second: Blob) -> BoundRange {
    first.insert(0, first.len() as u8);
    second.insert(0, second.len() as u8);
    first.append(&mut second);
    subset_range(first)
}
fn prefixed_by_three_chunk_bound_range(
    mut first: Blob,
    mut second: Blob,
    mut third: Blob,
) -> BoundRange {
    first.insert(0, first.len() as u8);
    second.insert(0, second.len() as u8);
    first.append(&mut second);
    first.append(&mut third);
    subset_range(first)
}

fn subset_range(from: Blob) -> BoundRange {
    if from.is_empty() {
        (..).into()
    } else {
        let to = {
            let mut x = from.iter().rev().peekable();
            let mut y = vec![];
            while Some(&&u8::MAX) == x.peek() {
                y.push(x.next().unwrap());
            }
            x.next().map(|xp| {
                x.rev()
                    .map(ToOwned::to_owned)
                    .chain([xp + 1].into_iter())
                    .chain(y.into_iter().map(|_| 0).rev())
                    .collect()
            })
        };
        match to {
            Some(to) => (from..to).into(),
            None => (from..).into(),
        }
    }
}

// === TableName ===

// TODO: max 255 byte, min 8 byte
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableName(String);

impl From<TableName> for String {
    fn from(t: TableName) -> Self {
        t.0
    }
}

impl From<TableName> for Blob {
    fn from(t: TableName) -> Self {
        t.0.into()
    }
}

impl TryFrom<Blob> for TableName {
    type Error = String;
    fn try_from(blob: Blob) -> std::result::Result<Self, Self::Error> {
        Self::try_from(String::from_utf8(blob).map_err(|x| x.to_string())?)
    }
}

impl TryFrom<String> for TableName {
    type Error = String;
    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        if value.as_bytes().len() > u8::MAX as usize {
            Err("table name can't exceed 255 bytes".into())
        } else {
            Ok(Self(value))
        }
    }
}

impl TryFrom<&str> for TableName {
    type Error = String;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::try_from(String::from(value))
    }
}

impl Deref for TableName {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// === Key ===

// TODO : consider empty inner_key
#[derive(Clone, PartialEq, Debug)]
pub struct Key {
    pub stack_id: StackID,
    pub table_name: TableName,
    pub inner_key: Blob,
}

impl From<Key> for tikv_client::Key {
    fn from(k: Key) -> Self {
        // TODO: bytes disclimiantor
        let first = k.stack_id.get_bytes();
        let second = k.table_name.as_bytes();
        let third = &k.inner_key;
        tikv_key_from_3_chunk(first, second, third)
    }
}

impl TryFrom<tikv_client::Key> for Key {
    type Error = String;
    fn try_from(value: tikv_client::Key) -> std::result::Result<Self, Self::Error> {
        let (a, b, c) = three_chunk_try_from_tikv_key(value)?;
        Ok(Self {
            stack_id: a
                .try_into()
                .map_err(|_| "cant deserialize a to stack_id".to_string())?,
            table_name: b
                .try_into()
                .map_err(|_| "cant deserialize b to table_name".to_string())?,
            inner_key: c.into(),
        })
    }
}

// === Scan ===

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanTableList {
    Whole,
    ByStackID(StackID),
    ByTableNamePrefix(StackID, TableName),
}

impl From<ScanTableList> for BoundRange {
    fn from(s: ScanTableList) -> Self {
        match s {
            ScanTableList::Whole => prefixed_by_a_chunk_bound_range(TABLE_LIST.into()),
            ScanTableList::ByStackID(stackid) => prefixed_by_two_chunk_bound_range(
                TABLE_LIST.into(),
                stackid.get_bytes().to_owned().into(),
            ),
            ScanTableList::ByTableNamePrefix(stackid, tablename) => {
                prefixed_by_three_chunk_bound_range(
                    TABLE_LIST.into(),
                    stackid.get_bytes().to_owned().into(),
                    tablename.into(),
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scan {
    ByTableName(StackID, TableName),
    ByInnerKeyPrefix(StackID, TableName, Blob),
}

impl From<Scan> for BoundRange {
    fn from(s: Scan) -> Self {
        match s {
            Scan::ByTableName(stackid, tablename) => prefixed_by_two_chunk_bound_range(
                stackid.get_bytes().to_owned().into(),
                tablename.into(),
            ),
            Scan::ByInnerKeyPrefix(stackid, tablename, key) => prefixed_by_three_chunk_bound_range(
                stackid.get_bytes().to_owned().into(),
                tablename.into(),
                key,
            ),
        }
    }
}

// === IpAndPort ===

// TODO: support hostname (also in gossip as well)
#[derive(Deserialize, Clone)]
pub struct IpAndPort {
    pub address: IpAddr,
    pub port: u16,
}

impl From<IpAndPort> for String {
    fn from(value: IpAndPort) -> Self {
        format!("{}:{}", value.address, value.port)
    }
}

impl TryFrom<&str> for IpAndPort {
    type Error = String;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        let x: Vec<&str> = value.split(':').collect();
        if x.len() != 2 {
            Err("Cant parse".into())
        } else {
            Ok(IpAndPort {
                address: x[0].parse().map_err(|e| format!("{e}"))?,
                port: x[1].parse().map_err(|e| format!("{e}"))?,
            })
        }
    }
}

#[async_trait]
pub trait Db: Clone {
    async fn put_stack_manifest(&self, stack_id: StackID, tables: Vec<TableName>) -> Result<()>;
    async fn put(&self, key: Key, value: Value, is_atomic: bool) -> Result<()>;
    async fn get(&self, key: Key) -> Result<Option<Value>>;
    async fn delete(&self, key: Key, is_atomic: bool) -> Result<()>;

    async fn delete_by_prefix(
        &self,
        stack_id: StackID,
        table_name: TableName,
        prefix_user_key: Blob,
    ) -> Result<()>;

    async fn clear_table(&self, stack_id: StackID, table_name: TableName) -> Result<()>;
    async fn scan(&self, scan: Scan, limit: u32) -> Result<Vec<(Key, Value)>>;
    async fn scan_keys(&self, scan: Scan, limit: u32) -> Result<Vec<Key>>;

    async fn table_list(
        &self,
        stack_id: StackID,
        table_name_prefix: Option<TableName>,
    ) -> Result<Vec<TableName>>;

    async fn stack_id_list(&self) -> Result<Vec<StackID>>;

    async fn batch_delete<K>(&self, keys: K) -> Result<()>
    where
        K: IntoIterator<Item = Key> + Send;

    async fn batch_get<K>(&self, keys: K) -> Result<Vec<(Key, Value)>>
    where
        K: IntoIterator<Item = Key> + Send;

    async fn batch_put<P>(&self, pairs: P, is_atomic: bool) -> Result<()>
    where
        P: IntoIterator<Item = (Key, Value)> + Send;

    async fn batch_scan<S>(&self, scans: S, each_limit: u32) -> Result<Vec<(Key, Value)>>
    where
        S: IntoIterator<Item = Scan> + Send;

    async fn batch_scan_keys<S>(&self, scans: S, each_limit: u32) -> Result<Vec<Key>>
    where
        S: IntoIterator<Item = Scan> + Send;

    async fn compare_and_swap(
        &self,
        key: Key,
        previous_value: Option<Value>,
        new_value: Value,
    ) -> Result<(Option<Value>, bool)>;
}

#[cfg(test)]
mod test {
    use super::*;
    use std::ops::{Bound, RangeBounds};

    #[test]
    fn subset_range_test() {
        let from = vec![0, 0, 0, 1];
        let res = subset_range(from.clone());
        let to = vec![0, 0, 0, 2];
        assert_eq!(res.start_bound(), Bound::Included(&from.into()));
        assert_eq!(res.end_bound(), Bound::Excluded(&to.into()));

        let from = vec![0, 255, 255, 255];
        let res = subset_range(from.clone());
        let to = vec![1, 0, 0, 0];
        assert_eq!(res.start_bound(), Bound::Included(&from.into()));
        assert_eq!(res.end_bound(), Bound::Excluded(&to.into()));

        let from = vec![255, 255, 255, 255];
        let res = subset_range(from.clone());
        assert_eq!(res.start_bound(), Bound::Included(&from.into()));
        assert_eq!(res.end_bound(), Bound::Unbounded);

        let from = vec![];
        let res = subset_range(from.clone());
        assert_eq!(res.start_bound(), Bound::Unbounded);
        assert_eq!(res.end_bound(), Bound::Unbounded);
    }

    #[test]
    fn test_prefixed_by_two_chunk_bound_range() {
        let scan = prefixed_by_two_chunk_bound_range(vec![0, 1], vec![12, 12, 12]);
        let bound_range: BoundRange = scan.clone().into();
        assert_eq!(
            bound_range.start_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12].into())
        );
        assert_eq!(
            bound_range.end_bound(),
            Bound::Excluded(&vec![2, 0, 1, 3, 12, 12, 13].into())
        );
    }

    #[test]
    fn test_prefixed_by_three_chunk_bound_range() {
        let scan = prefixed_by_three_chunk_bound_range(vec![0, 1], vec![12, 12, 12], vec![20, 22]);
        let bound_range: BoundRange = scan.clone().into();
        assert_eq!(
            bound_range.start_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12, 20, 22].into())
        );
        assert_eq!(
            bound_range.end_bound(),
            Bound::Excluded(&vec![2, 0, 1, 3, 12, 12, 12, 20, 23].into())
        );
    }
}
