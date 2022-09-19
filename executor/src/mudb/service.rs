//! Statefull Service
//! purpose is provide statefull api

use super::{error::Result, manager::Manager, Config, Db, Error};

pub use super::{
    types::{DatabaseID, KeyFilter, TableDescription},
    Updater, ValueFilter,
};

pub type Key = String;
pub type Value = String;
pub type Item = (Key, Value);

#[derive(Debug, Clone)]
pub struct Service(Manager);

impl Service {
    pub async fn new() -> Result<Self> {
        Ok(Self(Manager::new().await?))
    }

    fn manager(&self) -> &Manager {
        &self.0
    }

    /// clear all databases and make `./mudb` clean
    /// usefull in tests
    pub async fn clean(&self) -> Result<()> {
        let list = self.manager().query_db_by_prefix("")?;
        for name in list {
            self.manager().drop_db(&name).await?;
        }

        Ok(())
    }

    // manager stuff

    pub async fn create_db(&self, conf: Config) -> Result<()> {
        self.manager().create_db(conf.into()).await
    }

    pub async fn create_db_if_not_exist(&self, conf: Config) -> Result<()> {
        let x = self.create_db(conf).await;
        match x {
            Err(Error::DbAlreadyExist(_)) => Ok(()),
            x => x,
        }
    }

    pub async fn create_db_with_default_config(&self, database_id: DatabaseID) -> Result<()> {
        let conf = Config {
            database_id,
            ..Default::default()
        };
        self.create_db(conf).await
    }

    pub async fn create_db_with_default_config_if_not_exist(
        &self,
        database_id: DatabaseID,
    ) -> Result<()> {
        let conf = Config {
            database_id,
            ..Default::default()
        };
        self.create_db_if_not_exist(conf).await
    }

    pub async fn drop_db(&self, database_id: &DatabaseID) -> Result<()> {
        self.manager().drop_db(&database_id.to_string()).await
    }

    pub fn query_db_by_prefix(&self, prefix: &str) -> Result<Vec<DatabaseID>> {
        Ok(self
            .manager()
            .query_db_by_prefix(prefix)?
            .into_iter()
            .map(|s| s.parse().unwrap())
            .collect())
    }

    pub fn is_db_exists(&self, database_id: &DatabaseID) -> Result<bool> {
        self.manager().is_db_exists(&database_id.to_string())
    }

    pub fn get_db_conf(&self, name: &str) -> Result<Option<Config>> {
        Ok(self
            .manager()
            .get_db_conf(name)?
            .map(|c| c.try_into().unwrap()))
    }

    pub async fn cached_db_names(&self) -> Result<Vec<String>> {
        Ok(self.manager().get_cache().await?.into_keys().collect())
    }

    // db stuff

    async fn partial_run<T>(
        &self,
        database_id: DatabaseID,
        run: impl FnOnce(Db) -> Result<T> + Send + Sync + 'static,
    ) -> Result<T>
    where
        T: Send + Sync + 'static,
    {
        let db = self.manager().get_db(&database_id.to_string()).await?;
        ::tokio::task::spawn_blocking(move || run(db)).await?
    }

    pub async fn size_on_disk(&self, database_id: DatabaseID) -> Result<u64> {
        self.partial_run(database_id, move |db| db.size_on_disk())
            .await
    }

    pub async fn create_table(
        &self,
        database_id: DatabaseID,
        table_name: String,
    ) -> Result<TableDescription> {
        self.partial_run(database_id, move |db| {
            db.create_table(table_name.try_into()?).map(|(_, td)| td)
        })
        .await
    }

    pub async fn delete_table(
        &self,
        database_id: DatabaseID,
        table_name: String,
    ) -> Result<Option<TableDescription>> {
        self.partial_run(database_id, move |db| {
            db.delete_table(table_name.try_into()?)
        })
        .await
    }

    // table stuff

    pub async fn insert_one_item(
        &self,
        database_id: DatabaseID,
        table_name: String,
        key: Key,
        value: Value,
    ) -> Result<Key> {
        self.partial_run(database_id, move |db| {
            db.get_table(table_name.try_into()?)?
                .insert_one(key.into(), value.try_into()?)
                .map(Into::into)
        })
        .await
    }

    pub async fn find_item(
        &self,
        database_id: DatabaseID,
        table_name: String,
        key_filter: KeyFilter,
        value_filter: ValueFilter,
    ) -> Result<Vec<Item>> {
        self.partial_run(database_id, move |db| {
            Ok(db
                .get_table(table_name.try_into()?)?
                .find(key_filter, value_filter)?
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect())
        })
        .await
    }

    pub async fn update_item(
        &self,
        database_id: DatabaseID,
        table_name: String,
        key_filter: KeyFilter,
        value_filter: ValueFilter,
        update: Updater,
    ) -> Result<Vec<Item>> {
        self.partial_run(database_id, move |db| {
            Ok(db
                .get_table(table_name.try_into()?)?
                .update(key_filter, value_filter, update)?
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect())
        })
        .await
    }

    pub async fn delete_item(
        &self,
        database_id: DatabaseID,
        table_name: String,
        key_filter: KeyFilter,
        value_filter: ValueFilter,
    ) -> Result<Vec<Item>> {
        self.partial_run(database_id, move |db| {
            Ok(db
                .get_table(table_name.try_into()?)?
                .delete(key_filter, value_filter)?
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect())
        })
        .await
    }

    pub async fn delete_all_items(
        &self,
        database_id: DatabaseID,
        table_name: String,
    ) -> Result<()> {
        self.partial_run(database_id, move |db| {
            db.get_table(table_name.try_into()?)?.delete_all()
        })
        .await
    }

    pub async fn table_len(&self, database_id: DatabaseID, table_name: String) -> Result<usize> {
        self.partial_run(database_id, move |db| {
            Ok(db.get_table(table_name.try_into()?)?.len())
        })
        .await
    }

    pub async fn is_table_empty(
        &self,
        database_id: DatabaseID,
        table_name: String,
    ) -> Result<bool> {
        self.partial_run(database_id, move |db| {
            Ok(db.get_table(table_name.try_into()?)?.is_empty())
        })
        .await
    }
}
