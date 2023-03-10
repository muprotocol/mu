pub mod error;
mod types;

pub use self::types::{Blob, DeleteTable, Key, Scan, TableName};
use dyn_clonable::clonable;
use log::warn;
use mu_common::serde_support::TcpPortAddress;

use crate::{
    error::{Error, Result},
    types::*,
};
use anyhow::bail;
use async_trait::async_trait;
use mu_stack::StackID;
use serde::Deserialize;
use std::{collections::HashSet, fmt::Debug};
use tikv_client::{self, KvPair, RawClient, Value};
use tokio::time::{sleep, Duration};

// Only one of the fields should be provided
// Used struct instead of enum, only for better visual structure in config
#[derive(Deserialize, Clone)]
pub struct DbConfig {
    pub pd_addresses: Vec<TcpPortAddress>,
}

#[async_trait]
#[clonable]
pub trait DbClient: Send + Sync + Debug + Clone {
    async fn update_stack_tables(
        &self,
        stack_id: StackID,
        table_action_tuples: Vec<(TableName, DeleteTable)>,
    ) -> Result<()>;

    async fn get_raw(&self, key: Vec<u8>) -> Result<Option<Value>>;
    async fn scan_raw(
        &self,
        lower_inclusive: Vec<u8>,
        upper_exclusive: Vec<u8>,
        limit: u32,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    async fn put_raw(&self, key: Vec<u8>, value: Value, is_atomic: bool) -> Result<()>;
    async fn compare_and_swap_raw(
        &self,
        key: Vec<u8>,
        previous_value: Option<Value>,
        new_value: Value,
    ) -> Result<(Option<Value>, bool)>;
    async fn delete_raw(&self, key: Vec<u8>, is_atomic: bool) -> Result<()>;

    async fn get(&self, key: Key) -> Result<Option<Value>>;
    async fn put(&self, key: Key, value: Value, is_atomic: bool) -> Result<()>;
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

    async fn batch_put(&self, pairs: Vec<(Key, Value)>, is_atomic: bool) -> Result<()>;
    async fn batch_get(&self, keys: Vec<Key>) -> Result<Vec<(Key, Value)>>;
    async fn batch_delete(&self, keys: Vec<Key>) -> Result<()>;
    async fn batch_scan(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<(Key, Value)>>;
    async fn batch_scan_keys(&self, scans: Vec<Scan>, each_limit: u32) -> Result<Vec<Key>>;

    async fn table_list(
        &self,
        stack_id: StackID,
        table_name_prefix: Option<TableName>,
    ) -> Result<Vec<TableName>>;

    async fn stack_id_list(&self) -> Result<Vec<StackID>>;

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
    async fn stop(&self) -> anyhow::Result<()>;
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
    pub async fn new(endpoints: Vec<TcpPortAddress>) -> Result<Self> {
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
        table_action_tuples: Vec<(TableName, DeleteTable)>,
    ) -> Result<()> {
        // TODO: think of something for deleting existing tables
        let existing_tables = self
            .inner
            .scan_keys(types::ScanTableList::ByStackID(stack_id), 10000)
            .await?
            .into_iter()
            .map(|k| k.try_into().map_err(Into::into))
            .collect::<Result<HashSet<TableListKey>>>()?;

        let mut kvs_add = vec![];
        let mut kvs_delete = vec![];
        for (table, is_delete) in table_action_tuples {
            let k = TableListKey::new(stack_id, table);
            if !existing_tables.contains(&k) && !*is_delete {
                kvs_add.push((k, vec![]))
            } else if existing_tables.contains(&k) && *is_delete {
                kvs_delete.push(k)
            }
        }

        self.inner.batch_put(kvs_add).await?;
        self.inner.batch_delete(kvs_delete.clone()).await?;

        // TODO put this and batch_delete into transaction
        // we should do it and batch_delete atomic
        for delete in kvs_delete {
            self.clear_table(delete.stack_id, delete.table_name).await?
        }

        Ok(())
    }

    async fn get_raw(&self, key: Vec<u8>) -> Result<Option<Value>> {
        Ok(self.inner.get(key).await?)
    }

    async fn scan_raw(
        &self,
        lower_inclusive: Vec<u8>,
        upper_exclusive: Vec<u8>,
        limit: u32,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Ok(self
            .inner
            .scan(lower_inclusive..upper_exclusive, limit)
            .await?
            .into_iter()
            .map(|kv| (kv.0.into(), kv.1))
            .collect())
    }

    async fn put_raw(&self, key: Vec<u8>, value: Value, is_atomic: bool) -> Result<()> {
        Ok(self.get_inner(is_atomic).put(key, value).await?)
    }

    async fn compare_and_swap_raw(
        &self,
        key: Vec<u8>,
        previous_value: Option<Value>,
        new_value: Value,
    ) -> Result<(Option<Value>, bool)> {
        Ok(self
            .inner_atomic
            .compare_and_swap(key, previous_value, new_value)
            .await?)
    }

    async fn delete_raw(&self, key: Vec<u8>, is_atomic: bool) -> Result<()> {
        Ok(self.get_inner(is_atomic).delete(key).await?)
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

    // TODO change to delete_table and delete table_name from metadata too
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
    endpoints: Vec<TcpPortAddress>,
}

async fn ensure_cluster_healthy(
    endpoints: &Vec<TcpPortAddress>,
    max_try_count: u32,
) -> anyhow::Result<()> {
    #[tailcall::tailcall]
    async fn helper(
        endpoints: &Vec<TcpPortAddress>,
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

pub async fn start(db_config: DbConfig) -> anyhow::Result<Box<dyn DbManager>> {
    let endpoints = db_config.pd_addresses;
    ensure_cluster_healthy(&endpoints, 5).await?;
    Ok(Box::new(DbManagerImpl { endpoints }))
}

#[async_trait]
impl DbManager for DbManagerImpl {
    async fn make_client(&self) -> anyhow::Result<Box<dyn DbClient>> {
        Ok(Box::new(DbClientImpl::new(self.endpoints.clone()).await?))
    }

    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
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
