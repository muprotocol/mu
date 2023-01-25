use super::error::Result;
use async_trait::async_trait;
use bytes::BufMut;
use dyn_clonable::clonable;
use mu_stack::StackID;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::ops::Deref;
use tikv_client::{BoundRange, Key as TikvKey, Value};

// TODO: add constraint to Key (actually key.stack_id) to avoid this name
const TABLE_LIST_METADATA: &str = "__tlm";

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
    const E: &str = "Insufficient blobs to convert to Key";
    let split_at = |mut x: Vec<u8>, y| {
        if x.len() < y {
            Err(E.to_string())
        } else {
            let z = x.split_off(y);
            Ok((x, z))
        }
    };
    let split_first = |x: Vec<u8>| split_at(x, 1).map(|(mut x, y)| (x.pop().unwrap(), y));

    let x: Blob = value.into();

    let (a_size, x) = split_first(x)?;
    let (a, x) = split_at(x, a_size as usize)?;
    let (b_size, x) = split_first(x)?;
    let (b, c) = split_at(x, b_size as usize)?;

    Ok((a, b, c))
}

/// # TableListKey
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
        let first = TABLE_LIST_METADATA.as_bytes();
        // TODO: stack_id disclimiantor byte
        let second = k.stack_id.get_bytes();
        let third = k.table_name.as_bytes();
        tikv_key_from_3_chunk(first, second, third)
    }
}

impl TryFrom<tikv_client::Key> for TableListKey {
    type Error = String;
    fn try_from(value: tikv_client::Key) -> std::result::Result<Self, Self::Error> {
        let (a, b, c) = three_chunk_try_from_tikv_key(value)?;
        if TABLE_LIST_METADATA.as_bytes() != a.as_slice() {
            Err(format!(
                "cant deserialize TableListKey cause it dont have {TABLE_LIST_METADATA} part"
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

/// # TableName
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

// TODO : consider empty inner_key
/// # Key
#[derive(Clone, PartialEq, Eq, Debug)]
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
            inner_key: c,
        })
    }
}

/// # ScanTableList
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanTableList {
    Whole,
    ByStackID(StackID),
    ByTableNamePrefix(StackID, TableName),
}

impl From<ScanTableList> for BoundRange {
    fn from(s: ScanTableList) -> Self {
        match s {
            ScanTableList::Whole => prefixed_by_a_chunk_bound_range(TABLE_LIST_METADATA.into()),
            ScanTableList::ByStackID(stackid) => prefixed_by_two_chunk_bound_range(
                TABLE_LIST_METADATA.into(),
                stackid.get_bytes().to_owned().into(),
            ),
            ScanTableList::ByTableNamePrefix(stackid, tablename) => {
                prefixed_by_three_chunk_bound_range(
                    TABLE_LIST_METADATA.into(),
                    stackid.get_bytes().to_owned().into(),
                    tablename.into(),
                )
            }
        }
    }
}

/// # Scan
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

#[async_trait]
#[clonable]
pub trait DbClient: Send + Sync + Debug + Clone {
    async fn set_stack_manifest(&self, stack_id: StackID, tables: Vec<TableName>) -> Result<()>;
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
    async fn batch_delete(&self, keys: Vec<Key>) -> Result<()>;
    async fn batch_get(&self, keys: Vec<Key>) -> Result<Vec<(Key, Value)>>;
    async fn batch_put(&self, pairs: Vec<(Key, Value)>, is_atomic: bool) -> Result<()>;
    async fn batch_scan(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<(Key, Value)>>;
    async fn batch_scan_keys(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<Key>>;

    async fn compare_and_swap(
        &self,
        key: Key,
        previous_value: Option<Value>,
        new_value: Value,
    ) -> Result<(Option<Value>, bool)>;
}

#[async_trait]
pub trait DbManager: Send + Sync {
    async fn make_client(&self) -> anyhow::Result<Box<dyn DbClient>>;
    async fn stop_embedded_cluster(&self) -> anyhow::Result<()>;
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
        let res = subset_range(from);
        assert_eq!(res.start_bound(), Bound::Unbounded);
        assert_eq!(res.end_bound(), Bound::Unbounded);
    }

    #[test]
    fn test_prefixed_by_two_chunk_bound_range() {
        let scan = prefixed_by_two_chunk_bound_range(vec![0, 1], vec![12, 12, 12]);
        let bound_range: BoundRange = scan;
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
        let bound_range: BoundRange = scan;
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
