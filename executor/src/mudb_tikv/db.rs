use super::{
    embed_tikv::*,
    error::{Error, Result},
    types::*,
};
use crate::network::gossip::{KnownNodeConfig, NodeAddress};
use async_trait::async_trait;
use mu_stack::StackID;
use tikv_client::{self, KvPair, RawClient, Value};

// TODO: consider caching
// stacks_and_tables: HashMap<StackID, Vec<TableName>>,
#[derive(Clone)]
pub struct Db {
    inner: tikv_client::RawClient,
    tikv_runner: Box<dyn TikvRunner>,
}

impl Db {
    pub async fn new(
        node_address: NodeAddress,
        known_node_config: Vec<KnownNodeConfig>,
        config: TikvRunnerConfig,
    ) -> Result<Self> {
        Ok(Self {
            inner: RawClient::new(vec![config.pd.client_url.to_string()]).await?,
            tikv_runner: start(node_address, known_node_config, config).await?,
        })
    }

    fn atomic_or_not(&self, is_atomic: bool) -> Self {
        if is_atomic {
            Self {
                inner: self.inner.with_atomic_for_cas(),
                tikv_runner: self.tikv_runner.clone(),
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

#[derive(Clone)]
pub struct DummyTikvRunner;
#[async_trait]
impl TikvRunner for DummyTikvRunner {
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}
#[async_trait]
pub trait MudbReflection: Sized {
    async fn new_without_embed(endpoints: Vec<String>) -> Result<Self>;
}
#[async_trait]
impl MudbReflection for Db {
    async fn new_without_embed(endpoints: Vec<String>) -> Result<Self> {
        Ok(Self {
            inner: RawClient::new(endpoints).await?,
            tikv_runner: Box::new(DummyTikvRunner),
        })
    }
}
