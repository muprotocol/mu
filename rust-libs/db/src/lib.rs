pub mod error;
mod types;

pub use self::types::{Blob, Key, Scan, TableName};
pub use db_embedded_tikv::{IpAndPort, PdConfig, RemoteNode, TikvConfig, TikvRunnerConfig};
use dyn_clonable::clonable;
use log::warn;

use crate::{
    error::{Error, Result},
    types::*,
};
use anyhow::{bail, Context};
use async_trait::async_trait;
use db_embedded_tikv::*;
use mu_stack::StackID;
use serde::Deserialize;
use std::{collections::HashSet, fmt::Debug};
use tikv_client::{self, KvPair, RawClient, Value};
use tokio::time::{sleep, Duration};

// only one should be provided
// used struct instead of enum, only for better visual structure in config
#[derive(Deserialize, Clone)]
pub struct DbConfig {
    external: Option<Vec<IpAndPort>>,
    internal: Option<TikvRunnerConfig>,
}

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
#[clonable]
pub trait DbManager: Send + Sync + Clone {
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
    // TODO: VERY inefficient to create and drop connections continuously.
    // We need a connection pooling solution here.
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
        let existing_tables = self
            .inner
            .scan_keys(types::ScanTableList::ByStackID(stack_id), 10000)
            .await?
            .into_iter()
            .map(|k| k.try_into().map_err(Into::into))
            .collect::<Result<HashSet<TableListKey>>>()?;

        let mut kvs = vec![];
        for t in table_list {
            let k = TableListKey::new(stack_id, t);
            if !existing_tables.contains(&k) {
                kvs.push((k, vec![]))
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
            Some(prefix) => ScanTableList::ByTableName(stack_id, prefix),
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
struct DbManagerImpl {
    inner: Option<Box<dyn TikvRunner>>,
    endpoints: Vec<IpAndPort>,
}

pub async fn new_with_embedded_cluster(
    node_address: IpAndPort,
    known_node_config: Vec<RemoteNode>,
    config: TikvRunnerConfig,
) -> anyhow::Result<Box<dyn DbManager>> {
    let endpoints = vec![config.pd.advertise_client_url()];
    let inner = db_embedded_tikv::start(node_address, known_node_config, config).await?;

    match ensure_cluster_healthy(&endpoints, 10)
        .await
        .context("Timed out while trying to connect to TiKV cluster")
    {
        Err(e) => {
            inner
                .stop()
                .await
                .context("Failed to stop cluster after it failed to bootstrap")?;
            Err(e)
        }
        Ok(()) => Ok(Box::new(DbManagerImpl {
            endpoints,
            inner: Some(inner),
        })),
    }
}

pub async fn new_with_external_cluster(
    endpoints: Vec<IpAndPort>,
) -> anyhow::Result<Box<dyn DbManager>> {
    ensure_cluster_healthy(&endpoints, 5).await?;
    Ok(Box::new(DbManagerImpl {
        inner: None,
        endpoints,
    }))
}

async fn ensure_cluster_healthy(
    endpoints: &Vec<IpAndPort>,
    max_try_count: u32,
) -> anyhow::Result<()> {
    #[tailcall::tailcall]
    async fn helper(
        endpoints: &Vec<IpAndPort>,
        try_count: u32,
        max_try_count: u32,
    ) -> anyhow::Result<()> {
        // This call will not succeed unless the cluster is reachable and at least
        // N/2+1 PD nodes are already clustered.

        let check_cluster_health = || async {
            let client = DbClientImpl::new(endpoints.clone()).await?;
            client.inner.get(vec![]).await?;
            Result::Ok(())
        };

        match check_cluster_health().await {
            Err(e) if try_count < max_try_count => {
                warn!("Failed to reach TiKV cluster due to: {e:?}");
                sleep(Duration::from_millis(
                    (1.5_f64.powf(try_count as f64) * 1000.0).round() as u64,
                ))
                .await;
                helper(endpoints, try_count + 1, max_try_count)
            }
            Err(e) => bail!(e),
            Ok(_) => Ok(()),
        }
    }

    helper(endpoints, 0, max_try_count).await
}

pub async fn start(
    node: IpAndPort,
    remote_nodes: Vec<RemoteNode>,
    db_config: DbConfig,
) -> anyhow::Result<Box<dyn DbManager>> {
    match (db_config.internal, db_config.external) {
        (Some(tikv_config), None) => {
            new_with_embedded_cluster(node, remote_nodes, tikv_config).await
        }
        (None, Some(endpoints)) => new_with_external_cluster(endpoints).await,
        _ => bail!(
            "Exactly one of external or internal keys should be present in TiKV configuration"
        ),
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
