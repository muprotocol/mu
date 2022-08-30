use serde::{Deserialize, Serialize};
use sled::{IVec, Tree};
use validator::Validate;

use super::{
    config::Config,
    error::{Error, Result},
    input::*,
    output::*,
    query::Filter,
    types::ToFromIVec,
};

const DB_LIST: &str = "db_list";

#[derive(Debug, Clone)]
pub struct MuDB {
    db_inner: sled::Db,
    tables_descriptions_table: sled::Tree,
}

impl MuDB {
    /// Opens a `MuDB` based on the provided config.
    pub fn open_db(conf: Config) -> Result<Self> {
        // TODO: make obvious that this takes a long time to run, cache open databases somewhere (in a mailbox?)
        let name = conf.name.clone();
        let sled_conf: sled::Config = conf.into();
        let db_inner = sled_conf.open()?;
        let tables_descriptions_table = db_inner.open_tree(TABLES_DESCRIPTIONS_TABLE)?;
        Self::add_db_to_list(&name)?;
        Ok(MuDB {
            db_inner,
            tables_descriptions_table,
        })
    }

    /// Attempts to exclusively open the database, failing if it already exists
    pub fn create_db(conf: Config) -> Result<Self> {
        // TODO: make obvious that this takes a long time to run, cache open databases somewhere (in a mailbox?)
        let name = conf.name.clone();
        let sled_conf: sled::Config = conf.into();
        let db_inner = sled_conf.create_new(true).open()?;
        let tables_descriptions_table = db_inner.open_tree(TABLES_DESCRIPTIONS_TABLE)?;
        Self::add_db_to_list(&name)?;
        Ok(MuDB {
            db_inner,
            tables_descriptions_table,
        })
    }

    pub fn create_db_with_default_config(name: String) -> Result<Self> {
        let mut conf = Config::default();
        conf.name = name;
        Self::create_db(conf)
    }

    pub fn delete_db(name: &str) -> Result<()> {
        let conf = Config {
            name: name.to_owned(),
            ..Default::default()
        };
        let sled_conf: sled::Config = conf.into();
        {
            let _db = sled_conf.temporary(true).open()?; // set temporary true, to remove db after drop it
        } // drop _db will remove it cuz of temporary
        sled::open(std::path::Path::new(&format!("./mudb/{}", DB_LIST)))?.remove(&name)?;
        Ok(())
    }

    pub fn db_exists(name: &str) -> Result<bool> {
        sled::open(std::path::Path::new(&format!("./mudb/{}", DB_LIST)))?
            .get(name)
            .map(|opt| opt.is_some())
            .map_err(Into::into)
    }

    // TODO
    pub fn query_databases_by_prefix(prefix: &str) -> Result<Vec<String>> {
        let mut list = vec![];
        let prefixed_list =
            sled::open(std::path::Path::new(&format!("./mudb/{}", DB_LIST)))?.scan_prefix(prefix);

        for res in prefixed_list {
            let (k_ivec, _) = res?;
            list.push(String::from_ivec(&k_ivec))
        }

        Ok(list)
    }

    fn add_db_to_list(name: &str) -> Result<()> {
        // TODO: refactor to use mudb instead sled, after string key.
        sled::open(std::path::Path::new(&format!("./mudb/{}", DB_LIST)))?.insert(name, "")?;
        Ok(())
    }

    /// Create new table otherwise return err table already exists
    pub fn create_table(&self, input: CreateTableInput) -> Result<CreateTableOutput> {
        input.validate()?;
        let CreateTableInput { table_name } = input;

        // create table if not exist otherwise just open it.
        self.db_inner.open_tree(table_name.clone())?;

        let table_description = TableDescription {
            table_name: table_name.clone(),
        };

        // save schema
        let table_description_json = serde_json::to_string(&table_description)?;
        // check and if table_schema was sets before or from another thread,
        // return err `TableAlreadyExist`
        self.tables_descriptions_table
            .compare_and_swap(
                table_name.clone(),
                None as Option<&[u8]>,
                Some(table_description_json.to_ivec()),
            )?
            .map_err(|_| Error::TableAlreadyExist(table_name))
            .map(|_| CreateTableOutput { table_description })
    }

    /// Delete table if existed and return true.
    pub fn delete_table(&self, input: DeleteTableInput) -> Result<DeleteTableOutput> {
        input.validate()?;
        let DeleteTableInput { table_name } = input;

        match (
            self.table_exists(&table_name)?,
            self.db_inner.drop_tree(&table_name)?,
        ) {
            (true, true) => {
                // remove schema
                self.tables_descriptions_table
                    .remove(&table_name)
                    .map(|opt| DeleteTableOutput {
                        table_description: Some(opt.unwrap().into()),
                    })
                    .map_err(Into::into)
            }
            _ => Ok(DeleteTableOutput {
                table_description: None,
            }),
        }
    }

    // TODO: make it public
    fn _table_names(&self) -> Result<Vec<String>> {
        let mut list = vec![];
        self.tables_descriptions_table
            .iter()
            .keys()
            .try_for_each(|r| -> Result<()> {
                let k = r?;
                list.push(String::from_ivec(&k));
                Ok(())
            })?;

        Ok(list)
    }

    // TODO: make it public
    fn table_exists(&self, table_name: &str) -> Result<bool> {
        self.tables_descriptions_table
            .contains_key(table_name)
            .map_err(Into::into)
    }

    /// Returns the number of elements in this tree.
    /// Beware: performs a full O(n) scan under the hood.
    pub fn table_len(&self, input: TableLenInput) -> Result<TableLenOutput> {
        let TableLenInput { table_name } = input;
        let len = self.db_inner.open_tree(table_name)?.len();
        Ok(TableLenOutput { len })
    }

    /// return Ok(true) if table contains no items.
    pub fn table_is_empty(&self, input: TableIsEmptyInput) -> Result<TableIsEmptyOutput> {
        let TableIsEmptyInput { table_name } = input;
        let is_empty = self.db_inner.open_tree(table_name)?.is_empty();
        Ok(TableIsEmptyOutput { is_empty })
    }

    // TODO: make it public
    /// Returns the on-disk size of the storage files for this database.
    fn _size_on_disk(&self) -> Result<u64> {
        self.db_inner.size_on_disk().map_err(Into::into)
    }

    /// Inserts new key-value and returns inserted key, otherwise
    /// returns `Err` if:
    /// - `input.key()` was some but table key is auto or
    /// - `input.key()` was some but already inserted or
    /// - `input.key()` was none but table key is not autoincrement
    pub fn insert_one_item(&self, input: InsertOneItemInput) -> Result<InsertOneItemOutput> {
        input.validate()?;
        let InsertOneItemInput {
            table_name,
            key,
            value,
        } = input;

        let tree = self.db_inner.open_tree(table_name)?;

        tree.compare_and_swap(key.to_ivec(), None as Option<&[u8]>, Some(&value[..]))?
            .map(|_| InsertOneItemOutput { key: key.clone() })
            .map_err(|_| Error::KeyAlreadyExist(key))
    }

    fn find_and_effect(
        tree: &Tree,
        key_filter: KeyFilter,
        filter: Option<&Filter>,
        mut effect: impl FnMut(String, &str, &mut serde_json::Value) -> Result<()>,
    ) -> Result<()> {
        let mut eval_and_effect = |k_ivec, v_ivec| -> Result<()> {
            let v_string = String::from_ivec(&v_ivec);
            let mut v_json = serde_json::from_str(&v_string)?;

            match filter {
                // not match filter
                Some(filter) if !filter.eval(&v_json) => Ok(()),
                // matched filter or no filter
                Some(_) | None => effect(k_ivec, &v_string, &mut v_json),
            }
        };

        match key_filter {
            KeyFilter::Exact(key) => {
                if let Some(v_ivec) = tree.get(&key)? {
                    eval_and_effect(key, v_ivec)?;
                }
                Ok(())
            }

            KeyFilter::Prefix(prefix) => {
                for item_res in tree.scan_prefix(prefix) {
                    let (k_ivec, v_ivec) = item_res?;
                    eval_and_effect(String::from_ivec(&k_ivec), v_ivec)?;
                }
                Ok(())
            }
        }
    }

    /// Selects items in a table and returns a list of selected items.
    pub fn find_item(&self, input: FindItemInput) -> Result<FindItemOutput> {
        input.validate()?;
        let FindItemInput {
            table_name,
            key_filter,
            filter,
        } = input;

        let tree = self.db_inner.open_tree(table_name)?;

        let mut items = vec![];

        Self::find_and_effect(&tree, key_filter, filter.as_ref(), |key, v_str, _| {
            items.push((key, v_str.to_owned()));
            Ok(())
        })?;

        Ok(FindItemOutput { items })
    }

    /// Updates all Values that match the specified filter and key for a Table.
    pub fn update_item(&self, input: UpdateItemInput) -> Result<UpdateItemOutput> {
        input.validate()?;
        let UpdateItemInput {
            table_name,
            key_filter,
            filter,
            update,
        } = input;

        let tree = self.db_inner.open_tree(table_name)?;

        let mut batch = sled::Batch::default();
        let mut items = vec![];

        Self::find_and_effect(&tree, key_filter, filter.as_ref(), |key, _, v_json| {
            let u_res = update.update(v_json);
            if !u_res.is_empty() {
                let new_v_string = v_json.to_string();
                batch.insert(key.to_ivec(), new_v_string.to_ivec());
                items.push((key, new_v_string));
            }
            Ok(())
        })?;

        tree.apply_batch(batch)?;
        Ok(UpdateItemOutput { items })
    }

    /// Deletes all Values that match the specified filter and key for a Table.
    pub fn delete_item(&self, input: DeleteItemInput) -> Result<DeleteItemOutput> {
        input.validate()?;
        let DeleteItemInput {
            table_name,
            key_filter,
            filter,
        } = input;

        let tree = self.db_inner.open_tree(table_name)?;

        let mut batch = sled::Batch::default();
        let mut keys = vec![];

        Self::find_and_effect(&tree, key_filter, filter.as_ref(), |key, _, _| {
            batch.remove(key.to_ivec());
            keys.push(key);
            Ok(())
        })?;

        tree.apply_batch(batch)?;
        Ok(DeleteItemOutput { keys })
    }

    /// Deletes all table values.
    /// Note that this is not atomic.
    pub fn delete_all_items(&self, input: DeleteAllItemsInput) -> Result<DeleteAllItemsOutput> {
        input.validate()?;
        let DeleteAllItemsInput { table_name } = input;

        self.db_inner
            .open_tree(table_name)?
            .clear()
            .map_err(Into::into)
            .map(|_| DeleteAllItemsOutput)
    }

    // TODO
    // pub fn insert_many(&self, input: &InsertManyInput) -> Result<Vec<Key>> {
    //     let table_name = input.table_name();
    //     let items = input.items();
    //     let tree = self.db_inner.open_tree(&table_name)?;
    //     let auto_increment_id = self.auto_increment_id(&table_name)?;
    //     if auto_increment_id {
    //         return Err(Error::KeyIsAutoIncrement(
    //             "can't insert many in table with auto increment key".to_string(),
    //         ));
    //     }
    //     let mut batch = sled::Batch::default();
    //     for item in items {
    //         batch.insert(item.0.to_ivec(), item.1.to_ivec());
    //     }
    //     tree.apply_batch(batch)?;
    //     Ok(items.iter().map(|(key, _)| *key).collect())
    // }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TableDescription {
    pub table_name: String,
    // TODO
    // pub creation_date_time: DateTime,
}

impl From<IVec> for TableDescription {
    fn from(value: IVec) -> Self {
        serde_json::from_str::<TableDescription>(&String::from_ivec(&value)).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::super::query;
    use super::*;
    use assert_matches::assert_matches;
    use serde_json::json;
    use serial_test::serial;

    fn init_db() -> MuDB {
        let conf = Config {
            name: "test_db".to_string(),
            temporary: Some(true),
            ..Default::default()
        };
        MuDB::open_db(conf).unwrap()
    }

    fn init_table(db: &MuDB) -> (String, String) {
        // let input_handy_key = CreateTableInput::new("test_table_handy_key", false).unwrap();
        let input_1 = CreateTableInput {
            table_name: "test_table_handy_key".to_string(),
        };

        db.create_table(input_1.clone()).unwrap();

        let input_2 = CreateTableInput {
            table_name: "test_table_auto_key".to_string(),
        };

        db.create_table(input_2.clone()).unwrap();

        (input_1.table_name, input_2.table_name)
    }

    fn seed_item(db: &MuDB, table_name: &str) -> Vec<Item> {
        let mut items = vec![];

        for i in 1..4 {
            let value = json!({
                "num_item": i,
                "array_item": [1, 2, 3, 4],
                "obj_item": {
                    "in_1": "hello",
                    "in_2": "world",
                }
            })
            .to_string();

            let key = format!("ex::{}", i);
            let input = InsertOneItemInput {
                table_name: table_name.to_string(),
                key: key.clone(),
                value: value.clone(),
            };

            db.insert_one_item(input).unwrap();
            items.push((key, value));
        }

        for i in 1..4 {
            let value = json!("sth").to_string();
            let key = format!("other::{}", i);
            let input = InsertOneItemInput {
                table_name: table_name.to_string(),
                key: key.clone(),
                value: value.clone(),
            };

            db.insert_one_item(input).unwrap();
            items.push((key, value));
        }

        items
    }

    // test_db

    #[test]
    #[serial]
    fn delete_and_exists_db_test() {
        use std::fs;
        let conf = Config {
            name: "123456_db".to_string(),
            ..Default::default()
        };

        MuDB::open_db(conf).unwrap();

        let paths = fs::read_dir("./mudb").unwrap();
        assert!(paths
            .map(|res| res.unwrap())
            .any(|db_name| db_name.file_name().to_str() == Some("123456_db")));

        assert_eq!(MuDB::db_exists("123456_db").unwrap(), true);

        MuDB::delete_db("123456_db").unwrap();
        let paths = fs::read_dir("./mudb").unwrap();
        assert!(paths
            .map(|res| res.unwrap())
            .all(|db_name| db_name.file_name().to_str() != Some("123456_db")));

        assert_eq!(MuDB::db_exists("123456_db").unwrap(), false);
    }

    // TODO: query_databases_by_prefix
    // #[test]
    // #[serial]
    // fn query_db_by_prefix_r_ok_lists() {
    //     let conf_1 = Config {
    //         name: "a::b::db_1".to_string(),
    //         temporary: Some(true),
    //         ..Default::default()
    //     };
    //     let conf_2 = Config {
    //         name: "a::b::db_2".to_string(),
    //         ..conf_1.clone()
    //     };

    //     let conf_3 = Config {
    //         name: "a::c::db_3".to_string(),
    //         ..conf_1.clone()
    //     };

    //     let conf_4 = Config {
    //         name: "x::y::db_4".to_string(),
    //         ..conf_1.clone()
    //     };

    //     MuDB::create_db(conf_1).unwrap();
    //     MuDB::create_db(conf_2).unwrap();
    //     MuDB::create_db(conf_3).unwrap();
    //     MuDB::create_db(conf_4).unwrap();

    //     assert_eq!(
    //         MuDB::query_databases_by_prefix("a"),
    //         Ok(vec![
    //             "a::b::db_1".to_string(),
    //             "a::b::db_2".to_string(),
    //             "a::c::db_3".to_string()
    //         ])
    //     );

    //     assert_eq!(
    //         MuDB::query_databases_by_prefix("a::b"),
    //         Ok(vec!["a::b::db_1".to_string(), "a::b::db_2".to_string()])
    //     );
    // }

    // create_table

    #[test]
    #[serial]
    fn create_table_r_err_already_exist_w_happen() {
        let db = init_db();
        let input = CreateTableInput {
            table_name: "test_1".to_string(),
        };

        // create table -> ok
        let _ = db.create_table(input.clone()).unwrap();

        // duplicate create table -> err TableAlreadyExist
        let res = db.create_table(input.clone());

        assert_eq!(res, Err(Error::TableAlreadyExist(input.table_name)));
    }

    #[test]
    #[serial]
    fn create_table_r_err_input_validation_w_happen() {
        let db = init_db();

        let input = CreateTableInput {
            table_name: TABLES_DESCRIPTIONS_TABLE.to_string(),
        };

        let res = db.create_table(input);

        assert_matches!(res, Err(Error::InputValidationErr(_)));

        // empty string not valid
        let input = CreateTableInput {
            table_name: "".to_string(),
        };

        let res = db.create_table(input);

        assert_matches!(res, Err(Error::InputValidationErr(_)));
    }

    #[test]
    #[serial]
    fn create_table_r_ok_w_no_problem() {
        let db = init_db();
        let input = CreateTableInput {
            table_name: "table_1".to_string(),
        };

        let res = db.create_table(input.clone());
        assert_eq!(
            res,
            Ok(CreateTableOutput {
                table_description: TableDescription {
                    table_name: String::from("table_1"),
                }
            })
        );

        // check schema
        let table_descript: TableDescription = db
            .tables_descriptions_table
            .get(&input.table_name)
            .unwrap()
            .unwrap()
            .into();

        assert_eq!(
            table_descript,
            TableDescription {
                table_name: input.table_name,
            }
        )
    }

    // delete_table

    #[test]
    #[serial]
    fn delete_table_r_ok() {
        let db = init_db();
        let (handy_table, auto_table) = init_table(&db);
        let input = DeleteTableInput {
            table_name: handy_table.clone(),
        };
        let res = db.delete_table(input.clone());
        assert_eq!(
            res,
            Ok(DeleteTableOutput {
                table_description: Some(TableDescription {
                    table_name: handy_table,
                })
            })
        );
        assert_eq!(db._table_names(), Ok(vec![auto_table]));

        // again delete same table should return Ok(false)
        let res = db.delete_table(input);
        assert_eq!(
            res,
            Ok(DeleteTableOutput {
                table_description: None
            })
        );
    }

    // insert_one

    #[test]
    #[serial]
    fn insert_one_r_ok_inserted_key_w_no_problem() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);

        // insert into handy key table
        let input = InsertOneItemInput {
            table_name: table_handy_key,
            key: "ex::1".to_string(),
            value: "VALUE1".to_string(),
        };

        let res = db.insert_one_item(input);
        assert_eq!(
            res,
            Ok(InsertOneItemOutput {
                key: "ex::1".to_string()
            })
        );
    }

    #[test]
    #[serial]
    fn insert_one_r_err_input_validation_w_happen() {
        let db = init_db();
        let input = InsertOneItemInput {
            table_name: TABLES_DESCRIPTIONS_TABLE.to_string(),
            key: "ex::1".to_string(),
            value: "VALUE1".to_string(),
        };
        let res = db.insert_one_item(input);
        assert_matches!(res, Err(Error::InputValidationErr(_)));
    }

    #[test]
    #[serial]
    fn insert_one_r_err_key_already_exist_w_happen() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);

        let input = InsertOneItemInput {
            table_name: table_handy_key,
            key: "ex::1".to_string(),
            value: "VALUE1".to_string(),
        };

        let _ = db.insert_one_item(input.clone());
        let res = db.insert_one_item(input.clone());
        assert_eq!(res, Err(Error::KeyAlreadyExist(input.key)));
    }

    // find

    #[test]
    #[serial]
    fn find_r_ok_empty_w_not_match() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);
        let _ = seed_item(&db, &table_handy_key);

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({
                "hello": "null"
            }))),
        };

        let res = db.find_item(input);
        assert_eq!(res, Ok(FindItemOutput { items: vec![] }));

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({
                "num_item": 10
            }))),
        };
        let res = db.find_item(input);
        assert_eq!(res, Ok(FindItemOutput { items: vec![] }));

        let input = FindItemInput {
            table_name: table_handy_key,
            key_filter: KeyFilter::Exact("ex::2".to_string()),
            filter: Some(query::Filter(json!({
                "num_item": 1  // it's not ok for key:2
            }))),
        };
        let res = db.find_item(input);
        assert_eq!(res, Ok(FindItemOutput { items: vec![] }));
    }

    #[test]
    #[serial]
    fn find_r_ok_list_w_match() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);
        let items = seed_item(&db, &table_handy_key);

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({
                "array_item": [1, 2, 3, 4]
            }))),
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 3);

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({
                "obj_item": {
                    "in_1": "hello",
                }
            }))),
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 3);

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({}))),
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 3);

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({
                "num_item": 1
            }))),
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items.get(0), Some(&items[0]));

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Exact("ex::2".to_string()),
            filter: Some(query::Filter(json!({
                "num_item": 2
            }))),
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 1);
        assert_eq!(res.items.get(0), Some(&items[1]));

        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({
                "num_item": { "$in": [1, 2] }
            }))),
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 2);

        // find all ex prefixed
        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: None,
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 3);

        // find all ex other
        let input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("other".to_string()),
            filter: None,
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 3);

        // find all
        let input = FindItemInput {
            table_name: table_handy_key,
            key_filter: KeyFilter::Prefix("".to_string()),
            filter: None,
        };
        let res = db.find_item(input).unwrap();
        assert_eq!(res.items.len(), 6);
    }

    #[test]
    #[serial]
    #[should_panic(expected = "query error")]
    fn find_r_err_query_filter_w_invalid_filter() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);
        let _ = seed_item(&db, &table_handy_key);

        let input = FindItemInput {
            table_name: table_handy_key,
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: Some(query::Filter(json!({
                "hello": { "$in": 5 } // it should be and array
            }))),
        };

        let _ = db.find_item(input).expect("query error");
    }

    // update

    #[test]
    #[serial]
    fn update_r_ok_modified_items_w_happend() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);
        let _ = seed_item(&db, &table_handy_key);

        // With key and filter

        let key_filter = KeyFilter::Exact("ex::1".to_string());
        let filter = Some(query::Filter(json!({
            "num_item": 1
        })));

        let input = UpdateItemInput {
            table_name: table_handy_key.clone(),
            key_filter: key_filter.clone(),
            filter: filter.clone(),
            update: query::Update(json!({
                "$set": { "array_item.0": 10 },
                "$inc": { "array_item.1": 5 },
            })),
        };

        let updated_item = json!({
            "num_item": 1,
            "array_item": [10, 7, 3, 4],
            "obj_item": {
                "in_1": "hello",
                "in_2": "world",
            }
        })
        .to_string();

        let res = db.update_item(input);
        assert_eq!(
            res,
            Ok(UpdateItemOutput {
                items: vec![("ex::1".to_string(), updated_item.clone())]
            })
        );

        let f_input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter,
            filter,
        };
        assert_eq!(
            db.find_item(f_input),
            Ok(FindItemOutput {
                items: vec![("ex::1".to_string(), updated_item)]
            })
        );

        // Without key

        let key_filter = KeyFilter::Prefix("ex".to_string());
        let filter = Some(query::Filter(json!({
            "num_item": 2
        })));

        let input = UpdateItemInput {
            table_name: table_handy_key.clone(),
            key_filter: key_filter.clone(),
            filter: filter.clone(),
            update: query::Update(json!({
                "$set": { "array_item.0": 10 },
                "$inc": { "array_item.1": 5 },
            })),
        };

        let updated_item = json!({
            "num_item": 2,
            "array_item": [10, 7, 3, 4],
            "obj_item": {
                "in_1": "hello",
                "in_2": "world",
            }
        })
        .to_string();

        let res = db.update_item(input);
        assert_eq!(
            res,
            Ok(UpdateItemOutput {
                items: vec![("ex::2".to_string(), updated_item.clone())]
            })
        );

        let f_input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter,
            filter,
        };
        assert_eq!(
            db.find_item(f_input),
            Ok(FindItemOutput {
                items: vec![("ex::2".to_string(), updated_item)]
            })
        );

        // Multiple item

        let key_filter = KeyFilter::Prefix("ex".to_string());
        let filter = Some(query::Filter(json!({
            "obj_item": { "in_1": "hello" }
        })));

        let input = UpdateItemInput {
            table_name: table_handy_key.clone(),
            key_filter,
            filter: filter.clone(),
            update: query::Update(json!({
                "$set": { "obj_item.in_2": "you" },
                "$mul": { "num_item": 2 },
            })),
        };

        let res = db.update_item(input);
        let f_input = FindItemInput {
            table_name: table_handy_key,
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter,
        };
        assert_eq!(res.as_ref().unwrap().items.len(), 3);
        assert_eq!(res.unwrap().items, db.find_item(f_input).unwrap().items);
    }

    // delete

    #[test]
    #[serial]
    fn delete_r_ok_deleted_keys_w_happend() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);
        let _ = seed_item(&db, &table_handy_key);

        // With key and filter

        let key_filter = KeyFilter::Exact("ex::1".to_string());
        let filter = Some(query::Filter(json!({
            "num_item": 1
        })));

        let input = DeleteItemInput {
            table_name: table_handy_key.clone(),
            key_filter: key_filter.clone(),
            filter: filter.clone(),
        };

        let res = db.delete_item(input);
        assert_eq!(
            res,
            Ok(DeleteItemOutput {
                keys: vec!["ex::1".to_string()]
            })
        );

        let f_input = FindItemInput {
            table_name: table_handy_key.clone(),
            key_filter,
            filter,
        };
        assert_eq!(db.find_item(f_input), Ok(FindItemOutput { items: vec![] }));

        // Multiple item

        let filter = Some(query::Filter(json!({
            "obj_item": { "in_1": "hello" }
        })));

        let input = DeleteItemInput {
            table_name: table_handy_key.clone(),
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: filter.clone(),
        };

        let res = db.delete_item(input);
        assert_eq!(
            res,
            Ok(DeleteItemOutput {
                keys: vec!["ex::2".to_string(), "ex::3".to_string()]
            })
        );
        let f_input = FindItemInput {
            table_name: table_handy_key,
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter,
        };
        assert_eq!(db.find_item(f_input), Ok(FindItemOutput { items: vec![] }));
    }

    // delete all

    #[test]
    #[serial]
    fn delete_all_r_ok() {
        let db = init_db();
        let (table_handy_key, _) = init_table(&db);
        let _ = seed_item(&db, &table_handy_key);

        let input = DeleteAllItemsInput {
            table_name: table_handy_key.clone(),
        };

        let res = db.delete_all_items(input);
        assert_eq!(res, Ok(DeleteAllItemsOutput));

        let f_input = FindItemInput {
            table_name: table_handy_key,
            key_filter: KeyFilter::Prefix("ex".to_string()),
            filter: None,
        };
        assert_eq!(db.find_item(f_input), Ok(FindItemOutput { items: vec![] }));
    }
}
