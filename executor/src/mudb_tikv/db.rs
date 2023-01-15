use super::{
    embed_tikv::*,
    error::{Error, Result},
    types::*,
};
use crate::network::{gossip::KnownNodeConfig, NodeAddress};
use async_trait::async_trait;
use mu_stack::StackID;
use tikv_client::{self, KvPair, RawClient, Value};
use tokio::time::{sleep, Duration};

// TODO: caching
// * stacks_and_tables: HashMap<StackID, Vec<TableName>>,
#[derive(Clone)]
pub struct DbClientImpl {
    inner: tikv_client::RawClient,
    // tikv_runner: Option<Box<dyn TikvRunner>>,
}

impl DbClientImpl {
    pub async fn new(endpoints: Vec<IpAndPort>) -> Result<Self> {
        Ok(Self {
            inner: RawClient::new(endpoints).await?,
        })
    }

    fn make_atomic_or_do_nothing(&self, is_atomic: bool) -> Self {
        if is_atomic {
            Self {
                inner: self.inner.with_atomic_for_cas(),
            }
        } else {
            self.clone()
        }
    }

    /// Clear all data in tikv instant inside mudb
    /// Useful for test
    pub async fn clear_all_data(&self) -> Result<()> {
        self.inner.delete_range(..).await.map_err(Into::into)
    }
}

#[async_trait]
impl DbClient for DbClientImpl {
    async fn put_stack_manifest(
        &self,
        stack_id: StackID,
        table_list: Vec<TableName>,
    ) -> Result<()> {
        let s = ScanTableList::ByStackID(stack_id);
        self.inner.delete_range(s).await?;
        let kvs = table_list.into_iter().map(|t| {
            let k = TableListKey::new(stack_id, t);
            let v = "";
            (k, v)
        });
        self.inner.batch_put(kvs).await.map_err(Into::into)
    }

    async fn put(&self, key: Key, value: Value, is_atomic: bool) -> Result<()> {
        let k = TableListKey::new(key.stack_id, key.table_name.clone());
        match self.inner.get(k).await? {
            Some(_) => self
                .make_atomic_or_do_nothing(is_atomic)
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
        self.make_atomic_or_do_nothing(is_atomic)
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
            .map(|kv| kvpair_to_tuple(kv).unwrap())
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
            Some(prefix) => ScanTableList::ByTableNamePrefix(stack_id, prefix),
            None => ScanTableList::ByStackID(stack_id),
        };
        Ok(self
            .inner
            .scan_keys(scan, 128)
            .await?
            .into_iter()
            .map(|k| TableListKey::try_from(k).unwrap())
            .map(|k| k.table_name)
            .collect())
    }

    async fn stack_id_list(&self) -> Result<Vec<StackID>> {
        let scan = ScanTableList::Whole;
        Ok(self
            .inner
            .scan_keys(scan, 32)
            .await?
            .into_iter()
            .map(|k| TableListKey::try_from(k).unwrap())
            .map(|k| k.stack_id)
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
            .map(|kv| kvpair_to_tuple(kv).unwrap())
            .collect())
    }

    async fn batch_put<P>(&self, pairs: P, is_atomic: bool) -> Result<()>
    where
        P: IntoIterator<Item = (Key, Value)> + Send,
    {
        self.make_atomic_or_do_nothing(is_atomic)
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
            .map(|kv| kvpair_to_tuple(kv).unwrap())
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

#[derive(Clone)]
pub struct DbManagerImpl {
    inner: Option<Box<dyn TikvRunner>>,
    endpoints: Vec<IpAndPort>,
}

impl DbManagerImpl {
    pub async fn new_with_embedded_cluster(
        node_address: NodeAddress,
        known_node_config: Vec<KnownNodeConfig>,
        config: TikvRunnerConfig,
    ) -> anyhow::Result<Self> {
        let endpoints = vec![config.pd.client_url.clone()];
        let inner = Some(start(node_address, known_node_config, config).await?);
        Ok(Self { inner, endpoints })
    }

    pub async fn new_with_external_cluster(endpoints: Vec<IpAndPort>) -> Self {
        Self {
            inner: None,
            endpoints,
        }
    }
}

#[async_trait]
impl DbManager<DbClientImpl> for DbManagerImpl {
    async fn make_client(&self) -> anyhow::Result<DbClientImpl> {
        let mut x = DbClientImpl::new(self.endpoints.clone()).await;
        let mut i = 0;
        while x.is_err() && i < 5 {
            sleep(Duration::from_millis((i + 1) * 1000)).await;
            x = DbClientImpl::new(self.endpoints.clone()).await;
            i += 1;
        }
        Ok(x?)
    }
    async fn stop_embedded_cluster(&self) -> anyhow::Result<()> {
        match &self.inner {
            Some(r) => r.stop().await,
            None => Ok(()),
        }
    }
}

fn kvpair_to_tuple(kv: KvPair) -> std::result::Result<(Key, Value), String> {
    Ok((kv.key().clone().try_into()?, kv.into_value()))
}
