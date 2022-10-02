use std::collections::HashMap;

use super::{
    doc_filter::DocFilter,
    types::*,
    update::{Update, Updater},
    Error, Result,
};

#[derive(Debug, Clone)]
pub struct Table {
    inner: sled::Tree,
    // indexes: Indexes,
    /// primary key attribute
    pk_attr: String,
    /// secondary key trees
    sk_trees: HashMap<String, sled::Tree>,
}

impl Table {
    pub fn new(
        sk_trees: HashMap<String, sled::Tree>,
        pk_attr: String,
        inner: sled::Tree,
    ) -> Result<Self> {
        Ok(Self {
            inner,
            pk_attr,
            sk_trees,
        })
    }

    pub fn insert_one(&self, doc: Doc) -> Result<Key> {
        let pk = match doc.get(&self.pk_attr) {
            None => Err(Error::MissingIndexAttr(self.pk_attr.clone())),
            Some(x) => Key::try_from(x),
        }?;

        let x = self
            .query(
                KeyFilter::PK(KfBy::Exact(pk.clone().into())),
                DocFilter::none(),
            )?
            .pop();

        macro_rules! unique_insert {
            ($tree:expr, $key_attr:expr, $key:expr, $value:expr) => {
                $tree
                    .compare_and_swap(&$key, None as Option<&[u8]>, $value)?
                    .map_err(|_| Error::SecondaryKeyAlreadyExist($key_attr, $key))?;
            };
        }

        match x {
            None => self.update_sk_indexes(&doc, |sk_attr, sk, tree| {
                unique_insert!(tree, sk_attr.into(), sk, Some(pk.clone()));
                Ok(())
            }),
            Some((_, prev_doc)) => self.update_sk_indexes(&doc, |sk_attr, sk, tree| {
                let prev_sk = prev_doc
                    .get(sk_attr)
                    .map(|x| Key::try_from(x).unwrap())
                    .unwrap();
                tree.remove(prev_sk)?;
                unique_insert!(tree, sk_attr.into(), sk, Some(pk.clone()));
                Ok(())
            }),
        }?;

        self.inner
            .insert(pk.clone(), doc)
            .map(|_| pk)
            .map_err(Into::into)
    }

    fn update_sk_indexes(
        &self,
        doc: &Doc,
        update: impl Fn(&str, Key, &sled::Tree) -> Result<()>,
    ) -> Result<()> {
        self.sk_trees.iter().try_for_each(|(sk_attr, tree)| {
            let sk = match doc.get(sk_attr) {
                None => Err(Error::MissingIndexAttr(sk_attr.clone())),
                Some(x) => Key::try_from(x),
            }?;
            update(sk_attr, sk, tree)?;
            Ok(())
        })
    }

    /// Selects items in a table and returns a list of selected items.
    pub fn query(&self, kf: KeyFilter, vf: DocFilter) -> Result<Vec<Item>> {
        self.query_then_fold(kf, vf, |mut acc: Vec<Item>, item| {
            acc.push(item);
            Ok(acc)
        })
    }

    /// Updates all Values that match the specified filter and key for a Table.
    pub fn update(&self, kf: KeyFilter, vf: DocFilter, updater: Updater) -> Result<Vec<Item>> {
        let affected_indexes = updater.affect_attributes(self.all_indexes());
        if affected_indexes.is_empty() {
            let (items, batch) =
                self.query_then_fold(kf, vf, |mut acc: (Vec<Item>, sled::Batch), (k, v)| {
                    let (uv, changes) = v.update(&updater);
                    if !changes.is_empty() {
                        acc.0.push((k.clone(), uv.clone()));
                        acc.1.insert(k, uv);
                    }
                    Ok(acc)
                })?;

            self.inner.apply_batch(batch)?;
            Ok(items)
        } else {
            Err(Error::IndexAttrCantUpdate(affected_indexes.into()))
        }
    }

    fn all_indexes(&self) -> Vec<String> {
        let mut x = self.sk_trees.keys().map(Clone::clone).collect::<Vec<_>>();
        x.push(self.pk_attr.clone());
        x
    }

    /// Deletes all Values that match the specified filter and key for a Table.
    pub fn delete(&self, kf: KeyFilter, vf: DocFilter) -> Result<Vec<Item>> {
        let (items, batch) =
            self.query_then_fold(kf, vf, |mut acc: (Vec<Item>, sled::Batch), (key, value)| {
                acc.0.push((key.clone(), value));
                acc.1.remove(key);
                Ok(acc)
            })?;

        self.inner.apply_batch(batch)?;
        Ok(items)
    }

    fn query_then_fold<T: Default>(
        &self,
        kf: KeyFilter,
        vf: DocFilter,
        fold: impl FnMut(T, Item) -> Result<T>,
    ) -> Result<T> {
        self.query_by_key(kf)?
            .into_iter()
            .filter(|(_, value)| vf.eval(value))
            .try_fold(T::default(), fold)
    }

    pub fn query_by_key(&self, kf: KeyFilter) -> Result<Vec<Item>> {
        match kf {
            KeyFilter::PK(KfBy::Exact(pk)) => self
                .get_doc(&pk)?
                .map_or(Ok(vec![]), |doc| Ok(vec![(pk.into(), doc)])),

            KeyFilter::PK(KfBy::Prefix(prefix)) => self.scan_pk_prefix(&prefix),

            KeyFilter::SK(sk_attr, kf_type) => self
                .get_pks_from_sk_filter(&sk_attr, kf_type)?
                .into_iter()
                .try_fold(vec![], |mut acc, pk| -> Result<_> {
                    let doc = self.get_doc(&pk)?.unwrap();
                    acc.push((pk, doc));
                    Ok(acc)
                }),
        }
    }

    fn get_doc<T: AsRef<[u8]>>(&self, key: T) -> Result<Option<Doc>> {
        let x = self.inner.get(key)?.map(|a| a.try_into().unwrap());
        Ok(x)
    }

    fn scan_pk_prefix(&self, prefix: &str) -> Result<Vec<Item>> {
        self.inner
            .scan_prefix(prefix)
            .try_fold(vec![], |mut acc, a| {
                let (key, value) = a?;
                acc.push((key.try_into().unwrap(), value.try_into().unwrap()));
                Ok(acc)
            })
    }

    fn get_pks_from_sk_filter(&self, sk_attr: &str, filter: KfBy) -> Result<Vec<Key>> {
        let x = (self.sk_trees.get(sk_attr), filter);
        match x {
            (None, _) => Err(Error::HaveNoIndexTree(sk_attr.into())),

            (Some(tree), KfBy::Exact(key)) => Ok(tree
                .get(key)?
                .map_or(vec![], |value| vec![Key::try_from(value).unwrap()])),

            (Some(tree), KfBy::Prefix(prefix)) => {
                tree.scan_prefix(prefix)
                    .try_fold(vec![], |mut acc, a| -> Result<_> {
                        let b = Key::try_from(a?.1).unwrap();
                        acc.push(b);
                        Ok(acc)
                    })
            }
        }
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
    const SK_ATTR_1: &str = "sk";
    const SK_ATTR_2: &str = "sk2";

    fn init_table() -> Table {
        let path = format!("./mudb/{}", MANAGER_DB);
        let path = std::path::Path::new(&path);
        let db = sled::Config::default()
            .path(path)
            .temporary(true)
            .open()
            .unwrap();

        let pk = PK_ATTR.into();
        let sk_list: Vec<String> = vec![SK_ATTR_1.into(), SK_ATTR_2.into()];
        let sk_trees = HashMap::from([
            (sk_list[0].clone(), db.open_tree(&sk_list[0]).unwrap()),
            (sk_list[1].clone(), db.open_tree(&sk_list[1]).unwrap()),
        ]);
        let tree = db.open_tree("test_1").unwrap();
        let table_1 = Table::new(sk_trees, pk, tree).unwrap();

        table_1
    }

    fn temp_data_1(i: i32) -> (String, Doc) {
        let id = format!("ex::{}", i);
        let sk = format!("a::{}", i);
        let sk2 = format!("b::{}", i);
        let value = json!({
            PK_ATTR: id,
            SK_ATTR_1: sk,
            SK_ATTR_2: sk2,
            "num_item": i,
            "array_item": [1, 2, 3, 4],
            "obj_item": {
                "in_1": "hello",
                "in_2": "world",
            }
        })
        .try_into()
        .unwrap();

        (id, value)
    }

    fn temp_data_2(i: i32) -> (String, Doc) {
        let id = format!("other::{}", i);
        let sk = format!("c::{}", i);
        let sk2 = format!("d::{}", i);
        let value = json!({
            PK_ATTR: id,
            SK_ATTR_1: sk,
            SK_ATTR_2: sk2,
            "a_field": "sth"
        })
        .try_into()
        .unwrap();

        (id, value)
    }

    fn seed_item(table: &Table) -> Vec<Item> {
        let mut items = vec![];

        for i in 1..4 {
            let (id, value) = temp_data_1(i);

            table.insert_one(value.clone()).unwrap();
            items.push((id.into(), value));
        }

        for i in 1..4 {
            let (id, value) = temp_data_2(i);

            table.insert_one(value.clone()).unwrap();
            items.push((id.into(), value));
        }

        items
    }

    fn get_value(table: &Table, key: &str) -> Option<Doc> {
        table.inner.get(key).unwrap().map(|x| x.try_into().unwrap())
    }

    // insert_one

    #[test]
    #[serial]
    fn insert_one_r_ok_inserted_key_w_no_problem() {
        let table = init_table();

        let pk_value = "ex::1";
        let doc = Doc::try_from(json!({
            PK_ATTR: pk_value,
            SK_ATTR_1: "sth",
            SK_ATTR_2: "sthels",
            "field_1": "VALUE1"
        }))
        .unwrap();
        let res = table.insert_one(doc.clone());
        assert_eq!(res, Ok(Key::from(pk_value)));
        assert_eq!(table.len(), 1);
        let res = get_value(&table, pk_value).unwrap();
        assert_eq!(res, doc);

        // new another data

        let pk_value = "ex::2";
        let doc = Doc::try_from(json!({
            PK_ATTR: pk_value,
            SK_ATTR_1: "aajj",
            SK_ATTR_2: "aajjkk",
            "field_1": "VALUE1"
        }))
        .unwrap();
        let res = table.insert_one(doc.clone());
        assert_eq!(res, Ok(Key::from(pk_value)));
        assert_eq!(table.len(), 2);
        let res = get_value(&table, pk_value).unwrap();
        assert_eq!(res, doc);

        // overwrite

        let pk_value = "ex::2";
        let old_doc = doc;
        let new_doc = Doc::try_from(json!({
            PK_ATTR: pk_value,
            SK_ATTR_1: "xx",
            SK_ATTR_2: "xxx",
            "field_1": "VALUE1"
        }))
        .unwrap();
        let res = table.insert_one(new_doc.clone());
        assert_eq!(res, Ok(Key::from(pk_value)));
        assert_eq!(table.len(), 2);
        let res = get_value(&table, pk_value).unwrap();
        assert_ne!(res, old_doc);
        assert_eq!(res, new_doc);
    }

    #[test]
    #[serial]
    fn insert_one_r_err_sk_already_exist_w_happen() {
        let table = init_table();

        let pk_value = "ex::1";
        let doc = Doc::try_from(json!({
            PK_ATTR: pk_value,
            SK_ATTR_1: "sth",
            SK_ATTR_2: "sthels",
            "field_1": "VALUE1"
        }))
        .unwrap();
        let res = table.insert_one(doc.clone());
        assert_eq!(res, Ok(Key::from(pk_value)));
        assert_eq!(table.len(), 1);

        let pk_value_2 = "ex::2";
        let old_doc = doc;
        let new_doc = Doc::try_from(json!({
            PK_ATTR: pk_value_2,
            SK_ATTR_1: "sth",
            SK_ATTR_2: "aajjkk",
            "field_1": "VALUE1"
        }))
        .unwrap();
        let res = table.insert_one(new_doc.clone());
        assert_eq!(
            res,
            Err(Error::SecondaryKeyAlreadyExist(
                SK_ATTR_1.into(),
                "sth".into()
            ))
        );
        assert_eq!(table.len(), 1);
        let res = get_value(&table, pk_value_2);
        assert_eq!(res, None);
        let res = get_value(&table, pk_value).unwrap();
        assert_eq!(res, old_doc);
    }

    #[test]
    #[serial]
    fn insert_one_r_err_missing_index_attribute_w_happen() {
        let table = init_table();

        let id = "ex::1";
        let value = Doc::try_from(json!({
            PK_ATTR: id,
            SK_ATTR_1: "sth",
            // SK_ATTR_2 missing
            "field_1": "VALUE1"
        }))
        .unwrap();

        let res = table.insert_one(value.clone());
        assert_eq!(res, Err(Error::MissingIndexAttr(SK_ATTR_2.into())));
    }

    // query

    #[test]
    #[serial]
    fn query_r_ok_empty_w_not_match() {
        let table_1 = init_table();
        let _ = seed_item(&table_1);

        let res = table_1.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
            json!({
                "hello": "null"
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res, Ok(vec![]));

        let res = table_1.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
            json!({
                "num_item": 10
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res, Ok(vec![]));

        let res = table_1.query(
            KeyFilter::SK(SK_ATTR_1.into(), KfBy::Exact("c".into())),
            DocFilter::none(),
        );
        assert_eq!(res, Ok(vec![]));

        let res = table_1.query(
            KeyFilter::PK(KfBy::Prefix("ex::2".into())),
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
    fn query_r_ok_list_w_match() {
        let table = init_table();
        let items = seed_item(&table);

        let res = table.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
            json!({
                "array_item": [1, 2, 3, 4]
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
            json!({
                "obj_item": {
                    "in_1": "hello",
                }
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
            json!({}).try_into().unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
            json!({
                "num_item": 1
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert_eq!(res.unwrap().get(0), Some(&items[0]));

        let res = table.query(
            KeyFilter::PK(KfBy::Prefix("ex::2".into())),
            json!({
                "num_item": 2
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.as_ref().unwrap().len(), 1);
        assert_eq!(res.unwrap().get(0), Some(&items[1]));

        let res = table.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
            json!({
                "num_item": { "$in": [1, 2] }
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 2);

        let res = table.query(
            KeyFilter::SK(SK_ATTR_1.into(), KfBy::Prefix("a".into())),
            json!({
                "obj_item": {
                    "in_1": "hello",
                }
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table.query(
            KeyFilter::SK(SK_ATTR_2.into(), KfBy::Prefix("b".into())),
            json!({
                "obj_item": {
                    "in_1": "hello",
                }
            })
            .try_into()
            .unwrap(),
        );
        assert_eq!(res.unwrap().len(), 3);

        let res = table.query(
            KeyFilter::SK("sk2".into(), KfBy::Exact("b::1".into())),
            DocFilter::none(),
        );
        assert_eq!(res.unwrap().len(), 1);

        // query all ex prefixed
        let res = table.query(KeyFilter::PK(KfBy::Prefix("ex".into())), DocFilter::none());
        assert_eq!(res.unwrap().len(), 3);

        // query all ex other
        let res = table.query(
            KeyFilter::PK(KfBy::Prefix("other".into())),
            DocFilter::none(),
        );
        assert_eq!(res.unwrap().len(), 3);

        // query all
        let res = table.query(KeyFilter::PK(KfBy::Prefix("".into())), DocFilter::none());
        assert_eq!(res.unwrap().len(), 6);
    }

    #[test]
    #[serial]
    #[should_panic(expected = "filter error")]
    fn query_r_err_query_filter_w_invalid_filter() {
        let table_1 = init_table();
        let _ = seed_item(&table_1);

        let _ = table_1.query(
            KeyFilter::PK(KfBy::Prefix("ex".into())),
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
        let table_1 = init_table();
        let _ = seed_item(&table_1);

        // With key and filter

        let key_filter = KeyFilter::PK(KfBy::Exact("ex::1".into()));
        let filter = DocFilter::try_from(json!({
            "num_item": 1
        }))
        .unwrap();

        let updated_item = Doc::try_from(json!({
            PK_ATTR: "ex::1",
            SK_ATTR_1: "a::1",
            SK_ATTR_2: "b::1",
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

        let f_res = table_1.query(key_filter, filter);
        assert_eq!(f_res, Ok(vec![(Key::from("ex::1"), updated_item)]));

        // Without key

        let key_filter = KeyFilter::PK(KfBy::Prefix("ex".into()));
        let filter: DocFilter = json!({
            "num_item": 2
        })
        .try_into()
        .unwrap();

        let updated_item = Doc::try_from(json!({
            PK_ATTR: "ex::2",
            SK_ATTR_1: "a::2",
            SK_ATTR_2: "b::2",
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

        let f_res = table_1.query(key_filter, filter);
        assert_eq!(f_res, Ok(vec![(Key::from("ex::2"), updated_item)]));

        // Multiple item

        let key_filter = KeyFilter::PK(KfBy::Prefix("ex".into()));
        let filter: DocFilter = json!({
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

        let f_res = table_1.query(key_filter, filter);
        assert_eq!(res.unwrap(), f_res.unwrap());
    }

    #[test]
    #[serial]
    fn update_r_err_index_cant_update_w_happend() {
        let table = init_table();

        // single udpate & without seed

        let key_filter = KeyFilter::PK(KfBy::Exact("ex::1".into()));
        let res = table.update(
            key_filter.clone(),
            DocFilter::none(),
            json!({
                "$set": { PK_ATTR: "new_id" },
            })
            .try_into()
            .unwrap(),
        );

        assert_eq!(
            res,
            Err(Error::IndexAttrCantUpdate(vec![PK_ATTR.into()].into()))
        );

        // multiple update & with seed

        seed_item(&table);

        let res = table.update(
            key_filter,
            DocFilter::none(),
            json!({
                "$set": {
                    SK_ATTR_1: "new_id",
                    SK_ATTR_2: "sth"
                },
            })
            .try_into()
            .unwrap(),
        );

        assert_eq!(
            res,
            Err(Error::IndexAttrCantUpdate(
                vec![SK_ATTR_1.into(), SK_ATTR_2.into()].into()
            ))
        );
    }

    // delete

    #[test]
    #[serial]
    fn delete_r_ok_deleted_keys_w_happend() {
        let table_1 = init_table();
        let _ = seed_item(&table_1);

        // With key and filter

        let key_filter = KeyFilter::PK(KfBy::Exact("ex::1".into()));
        let filter: DocFilter = json!({
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

        let f_res = table_1.query(key_filter, filter);
        assert_eq!(f_res, Ok(vec![]));

        // Multiple item

        let filter: DocFilter = json!({
            "obj_item": { "in_1": "hello" }
        })
        .try_into()
        .unwrap();

        let res = table_1.delete(KeyFilter::PK(KfBy::Prefix("ex".into())), filter.clone());
        assert_eq!(
            res.unwrap()
                .into_iter()
                .map(|(k, _)| k)
                .collect::<Vec<Key>>(),
            vec![Key::from("ex::2"), Key::from("ex::3")]
        );

        let f_res = table_1.query(KeyFilter::PK(KfBy::Prefix("ex".into())), filter);
        assert_eq!(f_res, Ok(vec![]));
    }

    // delete all

    #[test]
    #[serial]
    fn delete_all_r_ok() {
        let table_1 = init_table();
        let _ = seed_item(&table_1);

        let res = table_1.delete_all();
        assert_eq!(res, Ok(()));

        let f_res = table_1.query(KeyFilter::PK(KfBy::Prefix("ex".into())), DocFilter::none());
        assert_eq!(f_res, Ok(vec![]));
    }
}
