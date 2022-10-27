use super::{
    config::ConfigInner,
    error::{Error, Result},
    table::Table,
    types::*,
    ValueFilter,
};

#[derive(Debug, Clone)]
pub struct Db {
    inner: sled::Db,
    pub id: String,
    pub conf: ConfigInner,
    // table_descriptions_table
    tdt: Table,
}

impl Db {
    /// Open new or existed Db
    pub fn open(conf: ConfigInner) -> Result<Self> {
        let inner: sled::Config = conf.clone().into();
        let inner: sled::Db = inner.open()?;
        // TODO: consider syncing tdt with sled::Db::tree_names
        let tdt = Table::new(inner.open_tree(TABLE_DESCRIPTIONS_TABLE)?);
        let id = conf.database_id.clone();
        Ok(Db {
            inner,
            tdt,
            id,
            conf,
        })
    }

    /// Create new table otherwise return err table already exists
    pub fn create_table(&self, table_name: TableNameInput) -> Result<(Table, TableDescription)> {
        // create table if not exist otherwise just open it.
        let table = Table::new(self.inner.open_tree(table_name.clone())?);

        let td = TableDescription {
            table_name: table_name.to_string(),
        };

        // save schema
        // check and if table_schema was sets before,
        // return err `TableAlreadyExist`
        self.tdt
            .insert_one(table_name.clone().into(), td.clone().into())
            .map_err(|_| Error::TableAlreadyExist(table_name.to_string()))
            .map(|_| (table, td))
    }

    pub fn get_table(&self, table_name: TableNameInput) -> Result<Table> {
        if !self.is_table_exists(table_name.clone())? {
            return Err(Error::TableDoseNotExist(table_name.into()));
        }
        let tree = self.inner.open_tree(table_name)?;
        Ok(Table::new(tree))
    }

    /// Delete table `TableDescription` if existed or `None` if not.
    pub fn delete_table(&self, table_name: TableNameInput) -> Result<Option<TableDescription>> {
        let is_table_exists = self.is_table_exists(table_name.clone())?;
        let is_dropped_success = self.inner.drop_tree(table_name.clone())?;

        if is_table_exists && is_dropped_success {
            self.tdt
                .delete(KeyFilter::Exact(table_name.into()), ValueFilter::none())
                .map(|vec| Some(vec[0].1.clone().try_into().unwrap()))
                .map_err(Into::into)
        } else {
            Ok(None)
        }
    }

    // TODO: maybe remove
    pub fn _table_names(&self) -> Result<Vec<String>> {
        Ok(self
            .tdt
            .find_by_key_filter(KeyFilter::Prefix("".into()))?
            .into_iter()
            .map(|(k, _)| k.into())
            .collect())
    }

    fn is_table_exists(&self, table_name: TableNameInput) -> Result<bool> {
        self.tdt.contains_key(table_name.into()).map_err(Into::into)
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

        fn init_and_seed() -> Self {
            let td = TestDb::init();
            td.create_table(TEST_TABLE.try_into().unwrap()).unwrap();
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
        let res = db.create_table(name.try_into().unwrap()).unwrap();
        assert_eq!(
            res.1,
            TableDescription {
                table_name: name.into()
            }
        );
    }

    #[test]
    #[serial]
    fn create_table_r_err_already_exist() {
        let db = TestDb::init_and_seed();

        let res = db.create_table(TEST_TABLE.try_into().unwrap());
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
