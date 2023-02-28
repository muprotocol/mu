use anyhow::{bail, Context, Error, Result};
use bytes::BufMut;
use mu_stack::StackID;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::ops::Deref;
use tikv_client::{BoundRange, Key as TikvKey};

const TABLE_LIST_METADATA: &str = "__tlm";

pub type Blob = Vec<u8>;

fn tikv_key_from_3_chunk(first: &[u8], second: &[u8], third: &[u8]) -> Blob {
    let mut x: Blob = Vec::with_capacity(first.len() + second.len() + third.len() + 2);
    assert!(first.len() <= u8::MAX as usize);
    x.push(first.len() as u8);
    x.put_slice(first);
    assert!(second.len() <= u8::MAX as usize);
    x.push(second.len() as u8);
    x.put_slice(second);
    x.put_slice(third);
    x
}

fn three_chunk_try_from_tikv_key(value: Blob) -> Result<(Blob, Blob, Blob)> {
    const E: &str = "Insufficient blobs to convert to Key";
    let split_at = |mut x: Vec<u8>, y| {
        if x.len() < y {
            bail!(E)
        } else {
            let z = x.split_off(y);
            Ok((x, z))
        }
    };
    let split_first = |x: Vec<u8>| split_at(x, 1).map(|(mut x, y)| (x.pop().unwrap(), y));

    let x = value;

    let (a_size, x) = split_first(x)?;
    let (a, x) = split_at(x, a_size as usize)?;
    let (b_size, x) = split_first(x)?;
    let (b, c) = split_at(x, b_size as usize)?;

    Ok((a, b, c))
}

#[derive(Clone, PartialEq, Eq, Hash)]
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

impl From<TableListKey> for TikvKey {
    fn from(k: TableListKey) -> Self {
        let first = TABLE_LIST_METADATA.as_bytes();
        let second = k.stack_id.to_bytes();
        let third = k.table_name.as_bytes();
        tikv_key_from_3_chunk(first, second.as_ref(), third).into()
    }
}

impl TryFrom<TikvKey> for TableListKey {
    type Error = Error;
    fn try_from(value: TikvKey) -> Result<Self> {
        let (a, b, c) = three_chunk_try_from_tikv_key(value.into())?;
        if TABLE_LIST_METADATA.as_bytes() != a.as_slice() {
            bail!("Can't deserialize TableListKey as it doesn't begin with {TABLE_LIST_METADATA}")
        } else {
            Ok(Self {
                stack_id: StackID::try_from_bytes(b.as_ref())
                    .context("Can't deserialize stack_id")?,
                table_name: c.try_into().context("Can't deserialize table_name")?,
            })
        }
    }
}

fn prefixed_by_a_chunk_bound_range(chunk: &[u8]) -> BoundRange {
    let mut buffer = Vec::with_capacity(chunk.len() + 1);
    buffer.push(chunk.len().try_into().unwrap());
    buffer.put_slice(chunk);
    subset_range(buffer)
}

fn prefixed_by_two_chunk_bound_range(first: &[u8], second: &[u8]) -> BoundRange {
    let mut buffer = Vec::with_capacity(first.len() + second.len() + 2);
    buffer.push(first.len().try_into().unwrap());
    buffer.put_slice(first);
    buffer.push(second.len().try_into().unwrap());
    buffer.put_slice(second);
    subset_range(buffer)
}

fn prefixed_by_three_chunk_bound_range(first: &[u8], second: &[u8], third: &[u8]) -> BoundRange {
    let mut buffer = Vec::with_capacity(first.len() + second.len() + 2);
    buffer.push(first.len().try_into().unwrap());
    buffer.put_slice(first);
    buffer.push(second.len().try_into().unwrap());
    buffer.put_slice(second);
    buffer.put_slice(third);
    subset_range(buffer)
}

fn subset_range(from: Blob) -> BoundRange {
    if from.is_empty() {
        (..).into()
    } else {
        let to = {
            let mut max = true;
            let mut to = Vec::with_capacity(from.len());
            for byte in from.iter().rev() {
                if max {
                    if *byte == u8::MAX {
                        to.push(0);
                    } else {
                        to.push(*byte + 1);
                        max = false;
                    }
                } else {
                    to.push(*byte);
                }
            }

            if max {
                None
            } else {
                to.reverse();
                Some(to)
            }
        };
        match to {
            Some(to) => (from..to).into(),
            None => (from..).into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    type Error = Error;
    fn try_from(blob: Blob) -> Result<Self> {
        Self::try_from(String::from_utf8(blob)?)
    }
}

impl TryFrom<String> for TableName {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        if value.as_bytes().len() > u8::MAX as usize {
            bail!("table name can't exceed 255 bytes")
        } else {
            Ok(Self(value))
        }
    }
}

impl TryFrom<&str> for TableName {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        Self::try_from(String::from(value))
    }
}

impl Deref for TableName {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteTable(pub bool);

impl Deref for DeleteTable {
    type Target = bool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Key {
    pub stack_id: StackID,
    pub table_name: TableName,
    pub inner_key: Blob,
}

impl From<Key> for Blob {
    fn from(k: Key) -> Self {
        let first = k.stack_id.to_bytes();
        let second = k.table_name.as_bytes();
        let third = &k.inner_key;
        tikv_key_from_3_chunk(first.as_ref(), second, third)
    }
}

impl From<Key> for TikvKey {
    fn from(k: Key) -> Self {
        Self::from(Blob::from(k))
    }
}

impl TryFrom<TikvKey> for Key {
    type Error = Error;
    fn try_from(value: TikvKey) -> Result<Self> {
        Self::try_from(Vec::from(value))
    }
}

impl TryFrom<Vec<u8>> for Key {
    type Error = Error;
    fn try_from(value: Vec<u8>) -> Result<Self> {
        let (a, b, c) = three_chunk_try_from_tikv_key(value)?;
        Ok(Self {
            stack_id: StackID::try_from_bytes(a.as_ref())
                .context("Can't deserialize first key chunk to a StackID")?,
            table_name: b
                .try_into()
                .context("Can't deserialize second key chunk to a string")?,
            inner_key: c,
        })
    }
}

/// # ScanTableList
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanTableList {
    Whole,
    ByStackID(StackID),
    ByTableName(StackID, TableName),
}

impl From<ScanTableList> for BoundRange {
    fn from(s: ScanTableList) -> Self {
        match s {
            ScanTableList::Whole => prefixed_by_a_chunk_bound_range(TABLE_LIST_METADATA.as_bytes()),
            ScanTableList::ByStackID(stackid) => prefixed_by_two_chunk_bound_range(
                TABLE_LIST_METADATA.as_bytes(),
                stackid.to_bytes().as_ref(),
            ),
            ScanTableList::ByTableName(stackid, table_name) => prefixed_by_three_chunk_bound_range(
                TABLE_LIST_METADATA.as_bytes(),
                stackid.to_bytes().as_ref(),
                table_name.as_bytes(),
            ),
        }
    }
}

// TODO: ByTableName is equal to ByInnerKeyPrefix with empty inner_key
// consider this type `Scan(StackID, TableName, Option<Blob>)`
/// # Scan
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scan {
    ByTableName(StackID, TableName),
    ByInnerKeyPrefix(StackID, TableName, Blob),
}

impl From<Scan> for BoundRange {
    fn from(s: Scan) -> Self {
        match s {
            Scan::ByTableName(stackid, table_name) => prefixed_by_two_chunk_bound_range(
                stackid.to_bytes().as_ref(),
                table_name.as_bytes(),
            ),
            Scan::ByInnerKeyPrefix(stackid, table_name, key) => {
                prefixed_by_three_chunk_bound_range(
                    stackid.to_bytes().as_ref(),
                    table_name.as_bytes(),
                    key.as_ref(),
                )
            }
        }
    }
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
        let scan = prefixed_by_two_chunk_bound_range(&[0, 1], &[12, 12, 12]);
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
        let scan = prefixed_by_three_chunk_bound_range(&[0, 1], &[12, 12, 12], &[20, 22]);
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
