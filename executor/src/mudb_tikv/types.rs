use super::error::Result;
use async_trait::async_trait;
use mu_stack::StackID;
use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::ops::Deref;
use tikv_client::{BoundRange, Value};

// TODO: add constrait to Key (acually key.stack_id) to avoid this name
// TODO: rename it
pub const TABLE_LIST: &str = "__tl";

pub type Blob = Vec<u8>;
pub trait KeyPart: Into<Blob> + TryFrom<Blob> + PartialEq + Clone + Send {}
impl<T> KeyPart for T where T: Into<Blob> + TryFrom<Blob> + PartialEq + Clone + Send {}

// === Abc Key ===

#[derive(Clone, PartialEq, Debug)]
pub struct AbcKey<A, B, C>
where
    A: KeyPart,
    B: KeyPart,
    C: KeyPart,
{
    pub a: A,
    pub b: B,
    pub c: C,
}

// TODO : change it to TryFrom due to below consideration
impl<A, B, C> From<AbcKey<A, B, C>> for tikv_client::Key
where
    A: KeyPart,
    B: KeyPart,
    C: KeyPart,
{
    fn from(key: AbcKey<A, B, C>) -> Self {
        let mut si: Blob = key.a.into();
        let mut tn: Blob = key.b.into();
        let mut ik: Blob = key.c.into();

        let mut x: Blob = vec![];
        // TODO: consider 255 max size
        x.push(si.len() as u8);
        x.append(&mut si);
        // TODO: consider 255 max size
        x.push(tn.len() as u8);
        x.append(&mut tn);
        x.append(&mut ik);
        x.into()
    }
}

impl<A, B, C> TryFrom<tikv_client::Key> for AbcKey<A, B, C>
where
    A: KeyPart,
    B: KeyPart,
    C: KeyPart,
{
    type Error = String;
    fn try_from(tkey: tikv_client::Key) -> std::result::Result<Self, Self::Error> {
        let e = "Insufficient blobs to convert to Key".to_string();
        let blob: Blob = tkey.into();
        let (a_size, r) = blob.split_first().ok_or(e.clone())?;
        let a_size = *a_size as usize;
        if r.len() < a_size {
            return Err(e);
        }
        let (a, r) = r.split_at(a_size);
        let (b_size, r) = r.split_at(1);
        let b_size = b_size[0] as usize;
        if r.len() < b_size {
            return Err(e);
        }
        let (b, c) = r.split_at(b_size);

        Ok(Self {
            a: Vec::from(a)
                .try_into()
                .map_err(|_| format!("cant deserialize a to {}", type_name::<A>()))?,
            b: Vec::from(b)
                .try_into()
                .map_err(|_| format!("cant deserialize b to {}", type_name::<B>()))?,
            c: Vec::from(c)
                .try_into()
                .map_err(|_| format!("cant deserialize c to {}", type_name::<C>()))?,
        })
    }
}

pub type TableListKey = AbcKey<StringKeyPart, StackID, TableName>;

// === Abc Scan ===

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbcScan<A, B, C>
where
    A: KeyPart,
    B: KeyPart,
    C: KeyPart,
{
    ByA(A),
    ByAb(A, B),
    ByAbc(A, B, C),
    ByAbCPrefix(A, B, C),
}

// TODO : change it to TryFrom due to below consideration
impl<A, B, C> From<AbcScan<A, B, C>> for BoundRange
where
    A: KeyPart,
    B: KeyPart,
    C: KeyPart,
{
    fn from(s: AbcScan<A, B, C>) -> Self {
        let prefixed_size = |mut x: Blob| {
            let mut y = vec![];
            // TODO: consider 255 max size
            y.push(x.len() as u8);
            y.append(&mut x);
            y
        };
        match s {
            AbcScan::ByA(a) => {
                let from = prefixed_size(a.into());
                subset_range(from)
            }
            AbcScan::ByAb(a, b) => {
                let from = {
                    let mut x = vec![];
                    x.append(&mut prefixed_size(a.into()));
                    x.append(&mut prefixed_size(b.into()));
                    x
                };
                subset_range(from)
            }
            AbcScan::ByAbc(a, b, c) => {
                let from = {
                    let mut x = vec![];
                    x.append(&mut prefixed_size(a.into()));
                    x.append(&mut prefixed_size(b.into()));
                    x.append(&mut c.into());
                    x
                };
                single_item_range(from)
            }
            AbcScan::ByAbCPrefix(a, b, c) => {
                let from = {
                    let mut x = vec![];
                    x.append(&mut prefixed_size(a.into()));
                    x.append(&mut prefixed_size(b.into()));
                    x.append(&mut c.into());
                    x
                };
                subset_range(from)
            }
        }
    }
}

pub type TableListScan = AbcScan<StringKeyPart, StackID, TableName>;

fn subset_range(from: Blob) -> BoundRange {
    // TODO: remove old
    // let from_p = {
    //     let mut x = from.clone();
    //     x.push(u8::MIN);
    //     x
    // };
    // let to = {
    //     let mut x = from;
    //     x.push(u8::MAX);
    //     x
    // };

    if from.is_empty() {
        (..).into()
    } else {
        let to = {
            let mut x: Blob = from
                .clone()
                .into_iter()
                .rev()
                .skip_while(|x| *x == 255)
                .collect();
            x.reverse();
            if let Some(xp) = x.pop() {
                x.push(xp + 1);
                let mut y: Blob = from
                    .iter()
                    .rev()
                    .take_while(|x| **x == 255)
                    .map(|_| 0)
                    .collect();
                y.reverse();
                x.append(&mut y);
                Some(x)
            } else {
                None
            }
        };
        match to {
            Some(to) => (from..to).into(),
            None => (from..).into(),
        }
    }
}

fn single_item_range(item: Blob) -> BoundRange {
    (item.clone()..=item).into()
}

// === TableName ===

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringKeyPart(String);

impl From<StringKeyPart> for String {
    fn from(t: StringKeyPart) -> Self {
        t.0
    }
}

impl From<StringKeyPart> for Blob {
    fn from(t: StringKeyPart) -> Self {
        t.0.into()
    }
}

impl TryFrom<Blob> for StringKeyPart {
    type Error = String;
    fn try_from(blob: Blob) -> std::result::Result<Self, Self::Error> {
        Ok(Self(String::from_utf8(blob).map_err(|x| x.to_string())?))
    }
}

impl From<String> for StringKeyPart {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for StringKeyPart {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl Deref for StringKeyPart {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// TODO: max 255 byte, min 8 byte
pub type TableName = StringKeyPart;

// === Key ===

// TODO : consider empty inner_key
#[derive(Clone, PartialEq, Debug)]
pub struct Key {
    pub stack_id: StackID,
    pub table_name: TableName,
    pub inner_key: Blob,
}

impl From<Key> for AbcKey<StackID, TableName, Blob> {
    fn from(k: Key) -> Self {
        Self {
            a: k.stack_id,
            b: k.table_name,
            c: k.inner_key,
        }
    }
}

impl From<AbcKey<StackID, TableName, Blob>> for Key {
    fn from(k: AbcKey<StackID, TableName, Blob>) -> Self {
        Self {
            stack_id: k.a,
            table_name: k.b,
            inner_key: k.c,
        }
    }
}

impl From<Key> for tikv_client::Key {
    fn from(k: Key) -> Self {
        AbcKey::from(k).into()
    }
}

impl TryFrom<tikv_client::Key> for Key {
    type Error = String;
    fn try_from(tik: tikv_client::Key) -> std::result::Result<Self, Self::Error> {
        Ok(AbcKey::try_from(tik)?.into())
    }
}

// === Scan ===

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scan {
    ByTableName(StackID, TableName),
    ByInnerKey(StackID, TableName, Blob),
    ByInnerKeyPrefix(StackID, TableName, Blob),
}

impl From<Scan> for AbcScan<StackID, TableName, Blob> {
    fn from(s: Scan) -> Self {
        match s {
            Scan::ByTableName(a, b) => AbcScan::ByAb(a, b),
            Scan::ByInnerKey(a, b, c) => AbcScan::ByAbc(a, b, c),
            Scan::ByInnerKeyPrefix(a, b, c) => AbcScan::ByAbCPrefix(a, b, c),
        }
    }
}

impl From<Scan> for BoundRange {
    fn from(s: Scan) -> Self {
        AbcScan::from(s).into()
    }
}

// === DatabaseManager === DbManagerCommone, DbManagerAtomic, AtomicDbManager, NoneAtomicDbManager,
// DbManager

#[async_trait]
pub trait DatabaseManager: Clone {
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

    async fn batch_scan<S>(&self, scanes: S, each_limit: u32) -> Result<Vec<(Key, Value)>>
    where
        S: IntoIterator<Item = Scan> + Send;

    async fn batch_scan_keys<S>(&self, scanes: S, each_limit: u32) -> Result<Vec<Key>>
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
        // TODO: remove old
        // let from = vec![0, 0, 0, 1, u8::MIN];
        // let to = vec![0, 0, 0, 1, u8::MAX];
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
    fn single_item_range_test() {
        let item = vec![0, 0, 0, 1];
        let res = single_item_range(item.clone());
        assert_eq!(res.start_bound(), Bound::Included(&item.clone().into()));
        assert_eq!(res.end_bound(), Bound::Included(&item.into()));
    }

    #[test]
    fn scan_by_table_name_to_bound_range() {
        let scan = AbcScan::<_, _, Blob>::ByAb(vec![0, 1], vec![12, 12, 12]);
        let bound_range: BoundRange = scan.clone().into();
        assert_eq!(
            bound_range.start_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12, u8::MIN].into())
        );
        assert_eq!(
            bound_range.end_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12, u8::MAX].into())
        );
    }

    #[test]
    fn scan_by_inner_key_prefix_to_bound_range() {
        let scan = AbcScan::ByAbCPrefix(vec![0, 1], vec![12, 12, 12], vec![20, 22]);
        let bound_range: BoundRange = scan.clone().into();
        assert_eq!(
            bound_range.start_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12, 20, 22, u8::MIN].into())
        );
        assert_eq!(
            bound_range.end_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12, 20, 22, u8::MAX].into())
        );
    }

    #[test]
    fn scan_by_inner_key_to_bound_range() {
        let scan = AbcScan::ByAbc(vec![0, 1], vec![12, 12, 12], vec![20, 22, 23]);
        let bound_range: BoundRange = scan.clone().into();
        assert_eq!(
            bound_range.start_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12, 20, 22, 23].into())
        );
        assert_eq!(
            bound_range.end_bound(),
            Bound::Included(&vec![2, 0, 1, 3, 12, 12, 12, 20, 22, 23].into())
        );
    }
}
