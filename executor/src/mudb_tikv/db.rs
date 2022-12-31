use std::net::{IpAddr, Ipv4Addr};

use super::{
    // embed_tikv::*,
    error::{Error, Result},
    types::*,
};
use crate::network::gossip::NodeAddress;
use async_trait::async_trait;
use mu_stack::StackID;
use tikv_client::{self, KvPair, RawClient, Value};

// TODO: consider caching
// stacks_and_tables: HashMap<StackID, Vec<TableName>>,
#[derive(Clone)]
pub struct Db {
    inner: tikv_client::RawClient,
    // TODO
    // tikv_runner: Option<Box<dyn TikvRunner>>,
}

impl Db {
    // TODO
    // pub async fn new(
    //     node_address: NodeAddress,
    //     gossip_seeds: &[NodeAddress],
    //     config: TikvRunnerConfig,
    // ) -> Result<Self> {
    //     Ok(Self {
    //         tikv_runner: Some(start(node_address, gossip_seeds, config).await?),
    //         inner: RawClient::new(config.pd.client_url.address).await?,
    //     })
    // }
    // TODO: change to use Hossian's code
    pub async fn new_test<S>(endpoints: Vec<S>) -> Result<Self>
    where
        S: Into<String> + Send,
    {
        let inner = RawClient::new(endpoints).await?;
        Ok(Self {
            inner,
            // TODO
            // tikv_runner: None,
        })
    }

    fn atomic_or_not(&self, is_atomic: bool) -> Self {
        if is_atomic {
            Self {
                inner: self.inner.with_atomic_for_cas(),
                // TODO
                // tikv_runner: self.tikv_runner.clone(),
            }
        } else {
            self.clone()
        }
    }

    /// Clear all data in tikv instant inside mudb
    /// Usefull for test
    pub async fn clear_all_data(&self) -> Result<()> {
        self.inner.delete_range(..).await.map_err(Into::into)
    }
}

#[async_trait]
impl DatabaseManager for Db {
    async fn put_stack_manifest(
        &self,
        stack_id: StackID,
        table_list: Vec<TableName>,
    ) -> Result<()> {
        let s = TableListScan::ByAb(TABLE_LIST.into(), stack_id);
        self.inner.delete_range(s).await?;
        let kvs = table_list.into_iter().map(|t| {
            let k = make_table_list_key(stack_id, t);
            let v = "";
            (k, v)
        });
        self.inner.batch_put(kvs).await.map_err(Into::into)
    }

    async fn put(&self, key: Key, value: Value, is_atomic: bool) -> Result<()> {
        // TODO: remove old
        // match self.stacks_and_tables.get(&key.stack_id) {
        //     Some(ts) if ts.contains(&key.table_name) => self
        //         .atomic_or_not(is_atomic)
        //         .inner
        //         .put(key, value)
        //         .await
        //         .map_err(Into::into),
        //     _ => Err(Error::StackIdOrTableDoseNotExist(key)),
        // }

        let k = make_table_list_key(key.stack_id.clone(), key.table_name.clone());
        match self.inner.get(k).await? {
            Some(_) => self
                .atomic_or_not(is_atomic)
                .inner
                .put(key, value)
                .await
                .map_err(Into::into),
            None => Err(Error::StackIdOrTableDoseNotExist(key)),
        }
    }

    async fn get(&self, key: Key) -> Result<Option<Value>> {
        self.inner.get(key).await.map_err(Into::into)
    }

    async fn delete(&self, key: Key, is_atomic: bool) -> Result<()> {
        self.atomic_or_not(is_atomic)
            .inner
            .delete(key)
            .await
            .map_err(Into::into)
    }

    async fn delete_by_prefix(
        &self,
        stack_id: StackID,
        table_name: TableName,
        prefix_inner_key: Blob,
    ) -> Result<()> {
        let scan = Scan::ByInnerKeyPrefix(stack_id, table_name, prefix_inner_key);
        self.inner.delete_range(scan).await.map_err(Into::into)
    }

    async fn clear_table(&self, stack_id: StackID, table_name: TableName) -> Result<()> {
        let scan = Scan::ByTableName(stack_id, table_name);
        self.inner.delete_range(scan).await.map_err(Into::into)
    }

    async fn scan(&self, scan: Scan, limit: u32) -> Result<Vec<(Key, Value)>> {
        Ok(self
            .inner
            .scan(scan, limit)
            .await?
            .into_iter()
            .map(kvpair_to_tuple)
            .collect())
    }

    async fn scan_keys(&self, scan: Scan, limit: u32) -> Result<Vec<Key>> {
        Ok(self
            .inner
            .scan_keys(scan, limit)
            .await?
            .into_iter()
            .map(|k| k.try_into().unwrap())
            .collect())
    }

    async fn table_list(
        &self,
        stack_id: StackID,
        table_name_prefix: Option<TableName>,
    ) -> Result<Vec<TableName>> {
        let scan = match table_name_prefix {
            Some(prefix) => TableListScan::ByAbCPrefix(
                StringKeyPart::from(TABLE_LIST),
                stack_id,
                StringKeyPart::from(prefix),
            ),
            None => TableListScan::ByAb(StringKeyPart::from(TABLE_LIST), stack_id),
        };
        Ok(self
            .inner
            .scan_keys(scan, 128)
            .await?
            .into_iter()
            .map(|k| TableListKey::try_from(k).unwrap())
            .map(|k| k.c)
            .collect())
    }

    async fn stack_id_list(&self) -> Result<Vec<StackID>> {
        let scan = TableListScan::ByA(TABLE_LIST.into());
        Ok(self
            .inner
            .scan_keys(scan, 32)
            .await?
            .into_iter()
            .map(|k| TableListKey::try_from(k).unwrap())
            .map(|k| k.b)
            .collect())
    }

    async fn batch_delete<K>(&self, keys: K) -> Result<()>
    where
        K: IntoIterator<Item = Key> + Send,
    {
        self.inner.batch_delete(keys).await.map_err(Into::into)
    }

    async fn batch_get<K>(&self, keys: K) -> Result<Vec<(Key, Value)>>
    where
        K: IntoIterator<Item = Key> + Send,
    {
        Ok(self
            .inner
            .batch_get(keys)
            .await?
            .into_iter()
            .map(kvpair_to_tuple)
            .collect())
    }

    async fn batch_put<P>(&self, pairs: P, is_atomic: bool) -> Result<()>
    where
        P: IntoIterator<Item = (Key, Value)> + Send,
    {
        self.atomic_or_not(is_atomic)
            .inner
            .batch_put(pairs)
            .await
            .map_err(Into::into)
    }

    async fn batch_scan<S>(&self, scanes: S, each_limit: u32) -> Result<Vec<(Key, Value)>>
    where
        S: IntoIterator<Item = Scan> + Send,
    {
        Ok(self
            .inner
            .batch_scan(scanes, each_limit)
            .await?
            .into_iter()
            .map(kvpair_to_tuple)
            .collect())
    }

    async fn batch_scan_keys<S>(&self, scanes: S, each_limit: u32) -> Result<Vec<Key>>
    where
        S: IntoIterator<Item = Scan> + Send,
    {
        Ok(self
            .inner
            .batch_scan_keys(scanes, each_limit)
            .await?
            .into_iter()
            .map(|k| k.try_into().unwrap())
            .collect())
    }

    async fn compare_and_swap(
        &self,
        key: Key,
        previous_value: Option<Value>,
        new_value: Value,
    ) -> Result<(Option<Value>, bool)> {
        self.inner
            .with_atomic_for_cas()
            .compare_and_swap(key, previous_value, new_value)
            .await
            .map_err(Into::into)
    }
}

fn make_table_list_key(stack_id: StackID, table_name: TableName) -> TableListKey {
    TableListKey {
        a: StringKeyPart::from(TABLE_LIST),
        b: stack_id,
        c: table_name,
    }
}

/// Just use it for Key or AbcKey<StackID, TableName, Blob> otherwise maybe panic
fn kvpair_to_tuple(kv: KvPair) -> (Key, Value) {
    (kv.key().clone().try_into().unwrap(), kv.into_value())
}

// TODO
// fn table_list_into_blob(tl: Vec<TableName>) -> Blob {
//     tl.into_iter()
//         .flat_map(|x| {
//             let mut x: Blob = x.into();
//             let mut y = vec![];
//             y.push(x.len() as u8);
//             y.append(&mut x);
//             y
//         })
//         .collect()
// }
// /// Just use it for `Blob` of `Vec<TableName>` otherwise maybe panic
// fn table_list_from_blob(b: Blob) -> Vec<TableName> {
//     let mut tl = vec![];
//     let mut b = b;
//     b.reverse();
//     while let Some(x_len) = b.pop() {
//         let mut x = b.split_off(b.len() - x_len as usize);
//         x.reverse();
//         let x: TableName = x.try_into().unwrap();
//         tl.push(x);
//     }
//     tl
// }

#[cfg(test)]
mod test {
    // use super::*;
    // use serial_test::serial;
    // use assert_matches::assert_matches;

    // async fn init() -> Db {
    //     Db::new(vec!["127.0.0.1:2379"]).await.unwrap()
    // }

    // fn stack_id() -> StackID {
    //     StackID::SolanaPublicKey([1; 32])
    // }

    // fn stack_id2() -> StackID {
    //     StackID::SolanaPublicKey([2; 32])
    // }

    // fn table_name_1() -> TableName {
    //     "a::a::a".into()
    // }

    // fn table_name_2() -> TableName {
    //     "a::a::b".into()
    // }

    // fn table_list() -> [TableName; 2] {
    //     [table_name_1(), table_name_2()]
    // }

    // TODO
    // #[tokio::test]
    // #[serial]
    // async fn put_stack_manifest_test() {
    //     let db = init().await;
    //     db.put_stack_manifest(stack_id(), table_list().into())
    //         .await
    //         .unwrap();
    //     db.put_stack_manifest(stack_id2(), table_list().into())
    //         .await
    //         .unwrap();
    //
    //     let res = db.stack_id_list().await.unwrap();
    //     assert_eq!(res, vec![stack_id(), stack_id2()]);
    //
    //     let res = db.table_list(stack_id(), None).await.unwrap();
    //     assert_eq!(res, table_list());
    // }
    //
    // #[test]
    // fn de_serialize_table_list() {
    //     let tl = vec![
    //         TableName::from("hello"),
    //         TableName::from("world"),
    //         TableName::from("how"),
    //         TableName::from("are"),
    //         TableName::from("you"),
    //     ];
    //
    //     let b: Blob = table_list_into_blob(tl.clone());
    //     let tl2: Vec<TableName> = table_list_from_blob(b);
    //     assert_eq!(tl, tl2);
    // }
}
