use std::collections::HashMap;

use super::{
    config::ConfigInner,
    doc_filter::DocFilter,
    error::{Error, Result},
    table::Table,
    types::*,
};

#[derive(Debug, Clone)]
pub struct Db {
    inner: sled::Db,
    pub id: String,
    pub conf: ConfigInner,
    /// table_descriptions_table
    td_table: Table,
}

impl Db {
    /// Open new or existed Db
    pub fn open(conf: ConfigInner) -> Result<Self> {
        let inner = sled::Config::from(conf.clone()).open()?;
        // TODO: consider syncing tdt with sled::Db::tree_names
        let indexes = Indexes {
            pk_attr: "table_name".into(),
            sk_attr_list: vec![],
        };
        let td_table = Self::open_table(&inner, &indexes, TABLE_DESCRIPTIONS_TABLE)?;
        let id = conf.database_id.clone();

        Ok(Db {
            inner,
            td_table,
            id,
            conf,
        })
    }

    fn open_table<T>(sled_db: &sled::Db, indexes: &Indexes, table_name: T) -> Result<Table>
    where
        T: AsRef<[u8]>,
    {
        let pk = indexes.pk_attr.clone();
        let sk_trees = indexes
            .sk_attr_list
            .iter()
            .try_fold(HashMap::new(), |mut acc, sk| {
                let tree = sled_db.open_tree(sk)?;
                acc.insert(sk.clone(), tree);
                Ok(acc) as Result<_>
            })?;

        let tree = sled_db.open_tree(table_name)?;
        Table::new(sk_trees, pk, tree)
    }

    /// Create new table otherwise return err table already exists
    pub fn create_table(
        &self,
        indexes: Indexes,
        table_name: TableNameInput,
    ) -> Result<(Table, TableDescription)> {
        if self.is_table_exists(table_name.clone())? {
            Err(Error::TableAlreadyExist(table_name.into()))
        } else {
            let table = Self::open_table(&self.inner, &indexes, &table_name)?;
            let td = TableDescription {
                table_name: table_name.to_string(),
                indexes,
            };
            // save table description
            self.td_table
                .insert_one(td.clone().into())
                .map(|_| (table, td))
        }
    }

    pub fn get_table(&self, table_name: TableNameInput) -> Result<Table> {
        if self.is_table_exists(table_name.clone())? {
            let indexes = self.table_description(table_name.clone())?.unwrap().indexes;
            Self::open_table(&self.inner, &indexes, table_name)
        } else {
            Err(Error::TableDoseNotExist(table_name.into()))
        }
    }

    pub fn get_or_create_table_if_not_exist(
        &self,
        indexes: Indexes,
        table_name: TableNameInput,
    ) -> Result<Table> {
        let x = self.create_table(indexes, table_name);
        match x {
            Ok((table, _)) => Ok(table),
            Err(Error::TableAlreadyExist(table)) => self.get_table(table.try_into().unwrap()),
            Err(e) => Err(e),
        }
    }

    /// Delete table `TableDescription` if existed or `None` if not.
    pub fn delete_table(&self, table_name: TableNameInput) -> Result<Option<TableDescription>> {
        let is_table_exists = self.is_table_exists(table_name.clone())?;
        let is_dropped_success = self.inner.drop_tree(table_name.clone())?;

        if is_table_exists && is_dropped_success {
            self.td_table
                .delete(
                    KeyFilter::PK(KfBy::Exact(table_name.into())),
                    DocFilter::none(),
                )
                .map(|vec| Some(vec[0].1.clone().try_into().unwrap()))
                .map_err(Into::into)
        } else {
            Ok(None)
        }
    }

    // TODO: maybe remove
    pub fn _table_names(&self) -> Result<Vec<String>> {
        let x = self
            .td_table
            .query_by_key(KeyFilter::PK(KfBy::Prefix("".into())))?
            .into_iter()
            .map(|(k, _)| k.into())
            .collect();

        Ok(x)
    }

    fn table_description(&self, table_name: TableNameInput) -> Result<Option<TableDescription>> {
        let x = self
            .td_table
            .query_by_key(KeyFilter::PK(KfBy::Exact(table_name.into())))?
            .pop()
            .map(|(_, td)| td.try_into().unwrap());

        Ok(x)
    }

    fn is_table_exists(&self, table_name: TableNameInput) -> Result<bool> {
        self.td_table
            .contains_key(table_name.into())
            .map_err(Into::into)
    }

    /// Returns the on-disk size of the storage files for this database.
    pub fn size_on_disk(&self) -> Result<u64> {
        self.inner.size_on_disk().map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assert_matches::assert_matches;
    use serial_test::serial;
    use std::ops::Deref;

    const TEST_TABLE: &str = "test_table";

    struct TestDb(Db);

    impl Deref for TestDb {
        type Target = Db;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for TestDb {
        // clear tables
        fn drop(&mut self) {
            let list = self._table_names().unwrap();

            for name in list {
                self.delete_table(name.to_string().try_into().unwrap())
                    .unwrap();
            }
        }
    }

    impl TestDb {
        fn init() -> Self {
            let conf = ConfigInner {
                database_id: "test_db".into(),
                ..Default::default()
            };

            Self(Db::open(conf).unwrap())
        }

        fn indexes() -> Indexes {
            let pk = "id".into();
            let sk = vec![];
            Indexes {
                pk_attr: pk,
                sk_attr_list: sk,
            }
        }

        fn init_and_seed() -> Self {
            let td = TestDb::init();
            td.create_table(Self::indexes(), TEST_TABLE.try_into().unwrap())
                .unwrap();

            td
        }
    }

    // TableNameInput

    #[test]
    #[serial]
    fn table_name_input_err() {
        let res = TableNameInput::try_from(TABLE_DESCRIPTIONS_TABLE);
        assert_matches!(res, Err(Error::InvalidTableName(_, _)))
    }

    #[test]
    #[serial]
    fn table_name_input_ok() {
        let res = TableNameInput::try_from("a_name");
        assert_matches!(res, Ok(_))
    }

    // create_table

    #[test]
    #[serial]
    fn create_table_r_ok_table_description() {
        let db = TestDb::init();
        let name = "create_table_test";
        let indexes = Indexes {
            pk_attr: "id".into(),
            sk_attr_list: vec![],
        };
        let res = db
            .create_table(indexes.clone(), name.try_into().unwrap())
            .unwrap();

        assert_eq!(
            res.1,
            TableDescription {
                table_name: name.into(),
                indexes
            }
        );

        assert!(db.is_table_exists(name.try_into().unwrap()).unwrap());
    }

    #[test]
    #[serial]
    fn create_table_r_err_already_exist() {
        let db = TestDb::init_and_seed();
        let indexes = Indexes {
            pk_attr: "id".into(),
            sk_attr_list: vec![],
        };
        let res = db.create_table(indexes, TEST_TABLE.try_into().unwrap());
        assert_eq!(res.err(), Some(Error::TableAlreadyExist(TEST_TABLE.into())));
    }

    // get_table

    #[test]
    #[serial]
    fn get_table_r_ok_table() {
        let db = TestDb::init_and_seed();

        let res = db.get_table(TEST_TABLE.try_into().unwrap());
        assert_matches!(res, Ok(Table { .. }));
    }

    #[test]
    #[serial]
    fn get_table_r_err_dose_not_exist() {
        let db = TestDb::init();

        let res = db.get_table(TEST_TABLE.try_into().unwrap());
        assert_eq!(res.err(), Some(Error::TableDoseNotExist(TEST_TABLE.into())));
    }

    // delete_table

    #[test]
    #[serial]
    fn delete_table_r_table_description() {
        let db = TestDb::init_and_seed();

        let res = db.delete_table(TEST_TABLE.try_into().unwrap());
        assert_eq!(
            res,
            Ok(Some(TableDescription {
                table_name: TEST_TABLE.into(),
                indexes: TestDb::indexes()
            }))
        );
    }

    #[test]
    #[serial]
    fn delete_table_r_none() {
        let db = TestDb::init();

        let res = db.delete_table(TEST_TABLE.try_into().unwrap());
        assert_eq!(res, Ok(None))
    }
}
