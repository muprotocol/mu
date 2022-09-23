use super::{types::*, Error, Result, Updater, ValueFilter};

#[derive(Debug, Clone)]
pub struct Table {
    inner: sled::Tree,
    indexes: Indexes,
}

impl Table {
    pub fn new(inner: sled::Tree, indexes: Indexes) -> Self {
        Self { inner, indexes }
    }

    pub fn insert_one(&self, value: Value) -> Result<Key> {
        // primary key attribute
        let pk_attr = &self.indexes.primary_key;
        let key_opt = value.get(pk_attr);
        let key = key_opt.map_or(
            Err(Error::MissingIndexAttribute(pk_attr.clone())),
            |key_v| Key::try_from(key_v),
        )?;

        // let key = Key::from()
        self.inner
            .compare_and_swap(key.clone(), None as Option<&[u8]>, Some(value))?
            .map(|_| key.clone())
            .map_err(|_| Error::PkAlreadyExist(pk_attr.into(), key))
    }

    fn partial_find<T: Default>(
        &self,
        kf: KeyFilter,
        vf: ValueFilter,
        fold: impl FnMut(T, Item) -> T,
    ) -> Result<T> {
        Ok(self
            .find_by_key_filter(kf)?
            .into_iter()
            .filter(|(_, value)| vf.eval(value))
            .fold(T::default(), fold))
    }

    pub fn find_by_key_filter(&self, kf: KeyFilter) -> Result<Vec<Item>> {
        match kf {
            KeyFilter::Exact(k) => match self.inner.get(k.clone())? {
                Some(v_ivec) => Ok(vec![(k.into(), v_ivec.try_into().unwrap())]),
                _ => Ok(vec![]),
            },

            KeyFilter::Prefix(prefix) => {
                self.inner
                    .scan_prefix(prefix)
                    .try_fold(vec![], |mut items, item_res| {
                        let (k_ivec, v_ivec) = item_res?;
                        items.push((k_ivec.try_into().unwrap(), v_ivec.try_into().unwrap()));
                        Ok(items)
                    })
            }
        }
    }

    /// Selects items in a table and returns a list of selected items.
    pub fn find(&self, kf: KeyFilter, vf: ValueFilter) -> Result<Vec<Item>> {
        self.partial_find(kf, vf, |mut acc: Vec<Item>, item| {
            acc.push(item);
            acc
        })
    }

    /// Updates all Values that match the specified filter and key for a Table.
    pub fn update(&self, kf: KeyFilter, vf: ValueFilter, updater: Updater) -> Result<Vec<Item>> {
        let (items, batch) =
            self.partial_find(kf, vf, |mut acc: (Vec<Item>, sled::Batch), (k, v)| {
                let (v, u_res) = v.update(&updater);
                if !u_res.is_empty() {
                    acc.0.push((k.clone(), v.clone()));
                    acc.1.insert(k, v);
                }
                acc
            })?;

        self.inner.apply_batch(batch)?;
        Ok(items)
    }

    /// Deletes all Values that match the specified filter and key for a Table.
    pub fn delete(&self, kf: KeyFilter, vf: ValueFilter) -> Result<Vec<Item>> {
        let (items, batch) =
            self.partial_find(kf, vf, |mut acc: (Vec<Item>, sled::Batch), (key, value)| {
                acc.0.push((key.clone(), value));
                acc.1.remove(key);
                acc
            })?;

        self.inner.apply_batch(batch)?;
        Ok(items)
    }

    /// Deletes all table values.
    /// Note that this is not atomic.
    pub fn delete_all(&self) -> Result<()> {
        self.inner.clear().map_err(Into::into)
    }

    /// Returns the number of elements in this tree.
    /// Beware: performs a full O(n) scan under the hood.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// return Ok(true) if table contains no items.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn contains_key(&self, key: Key) -> Result<bool> {
        self.inner.contains_key(key).map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;
    use serial_test::serial;

    const PK_ATTR: &str = "id";

    fn init_table() -> (Table, Table) {
        let path = format!("./mudb/{}", MANAGER_DB);
        let path = std::path::Path::new(&path);
        let db = sled::Config::default()
            .path(path)
            .temporary(true)
            .open()
            .unwrap();

        let inner = db.open_tree("test_1").unwrap();
        let primary_key = "id".to_string();
        let indexes = Indexes { primary_key };
        let table_1 = Table::new(inner, indexes);

        let inner = db.open_tree("test_2").unwrap();
        let primary_key = "id".to_string();
        let indexes = Indexes { primary_key };
        let table_2 = Table::new(inner, indexes);

        (table_1, table_2)
    }

    fn seed_item(table: &Table) -> Vec<Item> {
        let mut items = vec![];

        for i in 1..4 {
            let id = format!("ex::{}", i);
            let value: Value = json!({
                "id": id,
                "num_item": i,
                "array_item": [1, 2, 3, 4],
                "obj_item": {
                    "in_1": "hello",
                    "in_2": "world",
                }
            })
            .try_into()
            .unwrap();

            table.insert_one(value.clone()).unwrap();
            items.push((id.into(), value));
        }

        for i in 1..4 {
            let id = format!("other::{}", i);
            let value = Value::try_from(json!({
                "id": id,
                "a_field": "sth"
            }))
            .unwrap();

            table.insert_one(value.clone()).unwrap();
            items.push((id.into(), value));
        }

        items
    }

    // insert_one

    #[test]
    #[serial]
    fn insert_one_r_ok_inserted_key_w_no_problem() {
        let (table_1, _) = init_table();

        let value = Value::try_from(json!({
            "id": "ex::1",
            "field_1": "VALUE1"
        }))
        .unwrap();
        let res = table_1.insert_one(value);
        assert_eq!(res, Ok(Key::from("ex::1")));
    }

    #[test]
    #[serial]
    fn insert_one_r_err_key_already_exist_w_happen() {
        let (table_1, _) = init_table();

        let id = "ex::1";
        let value = Value::try_from(json!({
            "id": id,
            "field_1": "VALUE1"
        }))
        .unwrap();

        table_1.insert_one(value.clone()).unwrap();
        // again insert
        let res = table_1.insert_one(value.clone());
        assert_eq!(res, Err(Error::PkAlreadyExist(PK_ATTR.into(), id.into())));
    }

    // find

    #[test]
    #[serial]
    fn find_r_ok_empty_w_not_match() {
        let (table_1, _) = init_table();
        let _ = seed_item(&table_1);

        let res = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({
                "hello": "null"
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res, Ok(vec![]));

        let res = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({
                "num_item": 10
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res, Ok(vec![]));

        let res = table_1.find(
            KeyFilter::Prefix("ex::2".into()),
            json!({
                "num_item": 1  // it's not ok for key:2
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res, Ok(vec![]));
    }

    #[test]
    #[serial]
    fn find_r_ok_list_w_match() {
        let (table_1, _) = init_table();
        let items = seed_item(&table_1);

        let res = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({
                "array_item": [1, 2, 3, 4]
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({
                "obj_item": {
                    "in_1": "hello",
                }
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({}).try_into().unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({
                "num_item": 1
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert_eq!(res.unwrap().get(0), Some(&items[0]));

        let res = table_1.find(
            KeyFilter::Prefix("ex::2".into()),
            json!({
                "num_item": 2
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert_eq!(res.unwrap().get(0), Some(&items[1]));

        let res = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({
                "num_item": { "$in": [1, 2] }
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 2);

        // find all ex prefixed
        let res = table_1.find(KeyFilter::Prefix("ex".into()), ValueFilter::none());
        assert_eq!(res.unwrap().len(), 3);

        // find all ex other
        let res = table_1.find(KeyFilter::Prefix("other".into()), ValueFilter::none());
        assert_eq!(res.unwrap().len(), 3);

        // find all
        let res = table_1.find(KeyFilter::Prefix("".into()), ValueFilter::none());
        assert_eq!(res.unwrap().len(), 6);
    }

    #[test]
    #[serial]
    #[should_panic(expected = "filter error")]
    fn find_r_err_query_filter_w_invalid_filter() {
        let (table_1, _) = init_table();
        let _ = seed_item(&table_1);

        let _ = table_1.find(
            KeyFilter::Prefix("ex".into()),
            json!({
                "hello": { "$in": 5 } // it should be and array
            })
            .try_into()
            .expect("filter error"),
        );
    }

    // update

    #[test]
    #[serial]
    fn update_r_ok_modified_items_w_happend() {
        let (table_1, _) = init_table();
        let _ = seed_item(&table_1);

        // With key and filter

        let key_filter = KeyFilter::Exact("ex::1".into());
        let filter = ValueFilter::try_from(json!({
            "num_item": 1
        }))
        .unwrap();

        let updated_item = Value::try_from(json!({
            "id": "ex::1",
            "num_item": 1,
            "array_item": [10, 7, 3, 4],
            "obj_item": {
                "in_1": "hello",
                "in_2": "world",
            }
        }))
        .unwrap();

        let res = table_1.update(
            key_filter.clone(),
            filter.clone(),
            json!({
                "$set": { "array_item.0": 10 },
                "$inc": { "array_item.1": 5 },
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res, Ok(vec![(Key::from("ex::1"), updated_item.clone())]));

        let f_res = table_1.find(key_filter, filter);
        assert_eq!(f_res, Ok(vec![(Key::from("ex::1"), updated_item)]));

        // Without key

        let key_filter = KeyFilter::Prefix("ex".to_string());
        let filter: ValueFilter = json!({
            "num_item": 2
        })
        .try_into()
        .unwrap();

        let updated_item = Value::try_from(json!({
            "id": "ex::2",
            "num_item": 2,
            "array_item": [10, 7, 3, 4],
            "obj_item": {
                "in_1": "hello",
                "in_2": "world",
            }
        }))
        .unwrap();

        let res = table_1.update(
            key_filter.clone(),
            filter.clone(),
            json!({
                "$set": { "array_item.0": 10 },
                "$inc": { "array_item.1": 5 },
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res, Ok(vec![(Key::from("ex::2"), updated_item.clone())]));

        let f_res = table_1.find(key_filter, filter);
        assert_eq!(f_res, Ok(vec![(Key::from("ex::2"), updated_item)]));

        // Multiple item

        let key_filter = KeyFilter::Prefix("ex".to_string());
        let filter: ValueFilter = json!({
            "obj_item": { "in_1": "hello" }
        })
        .try_into()
        .unwrap();

        let res = table_1.update(
            key_filter.clone(),
            filter.clone(),
            json!({
                "$set": { "obj_item.in_2": "you" },
                "$mul": { "num_item": 2 },
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.as_ref().unwrap().len(), 3);

        let f_res = table_1.find(key_filter, filter);
        assert_eq!(res.unwrap(), f_res.unwrap());
    }

    // delete

    #[test]
    #[serial]
    fn delete_r_ok_deleted_keys_w_happend() {
        let (table_1, _) = init_table();
        let _ = seed_item(&table_1);

        // With key and filter

        let key_filter = KeyFilter::Exact("ex::1".into());
        let filter: ValueFilter = json!({
            "num_item": 1
        })
        .try_into()
        .unwrap();

        let res = table_1.delete(key_filter.clone(), filter.clone());
        assert_eq!(
            res.unwrap()
                .into_iter()
                .map(|(k, _)| k)
                .collect::<Vec<Key>>(),
            vec![Key::from("ex::1")]
        );

        let f_res = table_1.find(key_filter, filter);
        assert_eq!(f_res, Ok(vec![]));

        // Multiple item

        let filter: ValueFilter = json!({
            "obj_item": { "in_1": "hello" }
        })
        .try_into()
        .unwrap();

        let res = table_1.delete(KeyFilter::Prefix("ex".into()), filter.clone());
        assert_eq!(
            res.unwrap()
                .into_iter()
                .map(|(k, _)| k)
                .collect::<Vec<Key>>(),
            vec![Key::from("ex::2"), Key::from("ex::3")]
        );

        let f_res = table_1.find(KeyFilter::Prefix("ex".into()), filter);
        assert_eq!(f_res, Ok(vec![]));
    }

    // delete all

    #[test]
    #[serial]
    fn delete_all_r_ok() {
        let (table_1, _) = init_table();
        let _ = seed_item(&table_1);

        let res = table_1.delete_all();
        assert_eq!(res, Ok(()));

        let f_res = table_1.find(KeyFilter::Prefix("ex".into()), ValueFilter::none());
        assert_eq!(f_res, Ok(vec![]));
    }
}
