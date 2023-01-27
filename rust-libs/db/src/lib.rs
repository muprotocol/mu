pub mod error;
mod types;

pub use self::types::{Key, Scan, TableName};
pub use db_embedded_tikv::{
    IpAndPort, NodeAddress, PdConfig, RemoteNode, TikvConfig, TikvRunnerConfig,
};
use dyn_clonable::clonable;

use crate::{
    error::{Error, Result},
    types::*,
};
use anyhow::{bail, Context};
use async_trait::async_trait;
use db_embedded_tikv::*;
use mu_stack::StackID;
use std::fmt::Debug;
use tikv_client::{self, KvPair, RawClient, Value};
use tokio::time::{sleep, Duration};

#[async_trait]
#[clonable]
pub trait DbClient: Send + Sync + Debug + Clone {
    async fn update_stack_tables(&self, stack_id: StackID, tables: Vec<TableName>) -> Result<()>;

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

// TODO: caching
// * stacks_and_tables: HashMap<StackID, Vec<TableName>>,
#[derive(Clone)]
pub struct DbClientImpl {
    inner: tikv_client::RawClient,
    inner_atomic: tikv_client::RawClient,
}

impl Debug for DbClientImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbClientImpl").finish()
    }
}

impl DbClientImpl {
    pub async fn new(endpoints: Vec<IpAndPort>) -> Result<Self> {
        let new = RawClient::new(endpoints).await?;
        Ok(Self {
            inner: new.clone(),
            inner_atomic: new.with_atomic_for_cas(),
        })
    }

    fn get_inner(&self, atomic: bool) -> &RawClient {
        if atomic {
            &self.inner_atomic
        } else {
            &self.inner
        }
    }
}

#[async_trait]
impl DbClient for DbClientImpl {
    async fn update_stack_tables(
        &self,
        stack_id: StackID,
        table_list: Vec<TableName>,
    ) -> Result<()> {
        // TODO: think of something for deleting existing tables
        let mut kvs = vec![];
        for t in table_list {
            let k = TableListKey::new(stack_id, t);
            let v = "";
            if self.inner.get(k.clone()).await?.is_none() {
                kvs.push((k, v))
            }
        }

        self.inner.batch_put(kvs).await.map_err(Into::into)
    }

    async fn put(&self, key: Key, value: Value, is_atomic: bool) -> Result<()> {
        let k = TableListKey::new(key.stack_id, key.table_name.clone());
        match self.inner.get(k).await? {
            Some(_) => self
                .get_inner(is_atomic)
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
        self.get_inner(is_atomic)
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
        kv_pairs_to_tuples(self.inner.scan(scan, limit).await?)
    }

    async fn scan_keys(&self, scan: Scan, limit: u32) -> Result<Vec<Key>> {
        self.inner
            .scan_keys(scan, limit)
            .await?
            .into_iter()
            .map(|k| k.try_into().map_err(Error::InternalErr))
            .collect::<Result<Vec<Key>>>()
            .map_err(Into::into)
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
        self.inner
            .scan_keys(scan, 128)
            .await?
            .into_iter()
            .map(|k| {
                TableListKey::try_from(k)
                    .map(|x| x.table_name)
                    .map_err(Error::InternalErr)
            })
            .collect::<Result<Vec<TableName>>>()
            .map_err(Into::into)
    }

    async fn stack_id_list(&self) -> Result<Vec<StackID>> {
        self.inner
            .scan_keys(ScanTableList::Whole, 32)
            .await?
            .into_iter()
            .map(|k| {
                TableListKey::try_from(k)
                    .map(|x| x.stack_id)
                    .map_err(Error::InternalErr)
            })
            .collect::<Result<Vec<StackID>>>()
            .map_err(Into::into)
    }

    async fn batch_delete(&self, keys: Vec<Key>) -> Result<()> {
        self.inner.batch_delete(keys).await.map_err(Into::into)
    }

    async fn batch_get(&self, keys: Vec<Key>) -> Result<Vec<(Key, Value)>> {
        kv_pairs_to_tuples(self.inner.batch_get(keys).await?)
    }

    async fn batch_put(&self, pairs: Vec<(Key, Value)>, is_atomic: bool) -> Result<()> {
        self.get_inner(is_atomic)
            .batch_put(pairs)
            .await
            .map_err(Into::into)
    }

    async fn batch_scan(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<(Key, Value)>> {
        kv_pairs_to_tuples(self.inner.batch_scan(scans, each_limit).await?)
    }

    async fn batch_scan_keys(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<Key>> {
        self.inner
            .batch_scan_keys(scans, each_limit)
            .await?
            .into_iter()
            .map(|k| k.try_into().map_err(Error::InternalErr))
            .collect::<Result<Vec<Key>>>()
            .map_err(Into::into)
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
        known_node_config: Vec<RemoteNode>,
        config: TikvRunnerConfig,
    ) -> anyhow::Result<Self> {
        const TIMEOUT: u64 = 5;

        let endpoints = vec![config.pd.advertise_client_url()];
        let inner = db_embedded_tikv::start(node_address, known_node_config, config).await?;

        // wait 10 secs to ensure cluster is bootstrapped
        sleep(Duration::from_secs(10)).await;

        #[tailcall::tailcall]
        async fn go(
            inner: Box<dyn TikvRunner>,
            endpoints: Vec<IpAndPort>,
            timeout_count: u64,
        ) -> anyhow::Result<DbManagerImpl> {
            // try making client to ensure n/2 + 1 clusters have been bootstrapped
            match DbClientImpl::new(endpoints.clone()).await {
                Err(_) if timeout_count < TIMEOUT => {
                    sleep(Duration::from_secs(
                        1.5_f64.powf(timeout_count as f64) as u64
                    ))
                    .await;
                    go(inner, endpoints, timeout_count + 1)
                }
                Err(e) if timeout_count >= TIMEOUT => {
                    inner
                        .stop()
                        .await
                        .context("failed to stop cluster after failed to bootstrap")?;
                    bail!(e)
                }
                _ => Ok(DbManagerImpl {
                    inner: Some(inner),
                    endpoints,
                }),
            }
        }

        go(inner, endpoints, 0)
            .await
            .context("Timeout connection to PDs")
    }

    pub async fn new_with_external_cluster(endpoints: Vec<IpAndPort>) -> Self {
        Self {
            inner: None,
            endpoints,
        }
    }
}

#[async_trait]
impl DbManager for DbManagerImpl {
    async fn make_client(&self) -> anyhow::Result<Box<dyn DbClient>> {
        Ok(Box::new(DbClientImpl::new(self.endpoints.clone()).await?))
    }

    async fn stop_embedded_cluster(&self) -> anyhow::Result<()> {
        match &self.inner {
            Some(r) => r.stop().await,
            None => Ok(()),
        }
    }
}

fn kv_pairs_to_tuples(kv_pairs: Vec<KvPair>) -> Result<Vec<(Key, Value)>> {
    let kvpair_to_tuple = |x: KvPair| {
        Ok((
            x.key().clone().try_into().map_err(Error::InternalErr)?,
            x.into_value(),
        ))
    };

    kv_pairs
        .into_iter()
        .map(kvpair_to_tuple)
        .collect::<Result<Vec<(Key, Value)>>>()
        .map_err(Into::into)
}
