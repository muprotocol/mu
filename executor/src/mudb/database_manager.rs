//! Statefull Service
//! purpose is provide statefull api

use super::{agent::Agent, db::Db, error::Result, Config, Error};

pub use super::{
    doc_filter::DocFilter,
    types::{DatabaseID, Indexes, KeyFilter, KfBy, TableDescription},
    update::Updater,
};

pub type Key = String;
pub type Doc = String;

#[derive(Debug, Clone)]
pub struct DatabaseManager(Agent);

impl DatabaseManager {
    pub async fn new() -> Result<Self> {
        Ok(Self(Agent::new().await?))
    }

    fn manager(&self) -> &Agent {
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
        indexes: Indexes,
    ) -> Result<TableDescription> {
        self.partial_run(database_id, move |db| {
            db.create_table(table_name.try_into()?, indexes)
                .map(|(_, td)| td)
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
        doc: Doc,
    ) -> Result<Key> {
        self.partial_run(database_id, move |db| {
            db.get_table(table_name.try_into()?)?
                .insert_one(doc.try_into()?)
                .map(Into::into)
        })
        .await
    }

    pub async fn query(
        &self,
        database_id: DatabaseID,
        table_name: String,
        key_filter: KeyFilter,
        value_filter: DocFilter,
    ) -> Result<Vec<Doc>> {
        self.partial_run(database_id, move |db| {
            Ok(db
                .get_table(table_name.try_into()?)?
                .query(key_filter, value_filter)?
                .into_iter()
                .map(|(_, doc)| doc.into())
                .collect())
        })
        .await
    }

    pub async fn update_item(
        &self,
        database_id: DatabaseID,
        table_name: String,
        key_filter: KeyFilter,
        value_filter: DocFilter,
        updater: Updater,
    ) -> Result<Vec<Doc>> {
        self.partial_run(database_id, move |db| {
            Ok(db
                .get_table(table_name.try_into()?)?
                .update(key_filter, value_filter, updater)?
                .into_iter()
                .map(|(_, doc)| doc.into())
                .collect())
        })
        .await
    }

    pub async fn delete_item(
        &self,
        database_id: DatabaseID,
        table_name: String,
        key_filter: KeyFilter,
        value_filter: DocFilter,
    ) -> Result<Vec<Doc>> {
        self.partial_run(database_id, move |db| {
            Ok(db
                .get_table(table_name.try_into()?)?
                .delete(key_filter, value_filter)?
                .into_iter()
                .map(|(_, doc)| doc.into())
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
