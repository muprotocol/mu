use mu::mudb::{database_manager::*, Config, Error, Result};
use serde_json::json;
use serial_test::serial;

async fn find_and_update_again(
    database_manager: &DatabaseManager,
    database_id: &DatabaseID,
    table: &str,
) -> Result<()> {
    // find
    database_manager
        .query(
            database_id.clone(),
            table.into(),
            KeyFilter::PK(KfBy::Prefix("".into())),
            json!({
                "num_item": { "$lt": 5 },
                "array_item": [2, 3]
            })
            .try_into()
            .unwrap(),
        )
        .await?;

    // update
    database_manager
        .update_item(
            database_id.clone(),
            table.into(),
            KeyFilter::PK(KfBy::Exact("ex::1".into())),
            json!({
                "num_item": 1
            })
            .try_into()
            .unwrap(),
            json!({
                "$set": { "array_item.0": 10 },
                "$inc": { "array_item.1": 5 },
            })
            .try_into()
            .unwrap(),
        )
        .await?;

    Ok(())
}

const PK_ATTR: &str = "id";
const SK_ATTR: &str = "phone_number";
const TABLE_1_NAME: &str = "table_1";
const TABLE_2_NAME: &str = "table_2";

#[tokio::test]
#[serial]
async fn test_database_manager() {
    let database_manager = DatabaseManager::new().await.unwrap();

    // == init db ==

    let database_id = DatabaseID {
        db_name: "test_database_manager".into(),
        ..Default::default()
    };
    let conf = Config {
        database_id: database_id.clone(),
        ..Default::default()
    };
    database_manager.create_db(conf).await.unwrap();

    // == create table ==

    let indexes = Indexes {
        pk_attr: PK_ATTR.into(),
        sk_attr_list: vec![],
    };
    database_manager
        .create_table(database_id.clone(), TABLE_1_NAME.into(), indexes.clone())
        .await
        .unwrap();

    // == create table 2 ==

    let indexes_2 = Indexes {
        pk_attr: PK_ATTR.into(),
        sk_attr_list: vec![SK_ATTR.into()],
    };
    database_manager
        .create_table(database_id.clone(), TABLE_2_NAME.into(), indexes_2.clone())
        .await
        .unwrap();

    // == insert one item ==

    let doc = json!({
        PK_ATTR: "ex::1",
        "num_item": 1,
        "array_item": [1, 2, 3, 4],
        "obj_item": {
            "in_1": "hello",
            "in_2": "world",
        }
    })
    .to_string();
    let res1 = database_manager
        .insert_one_item(database_id.clone(), TABLE_1_NAME.into(), doc.clone())
        .await;
    assert_eq!(res1, Ok("ex::1".to_string()));

    // == find ==

    let res = database_manager
        .query(
            database_id.clone(),
            TABLE_1_NAME.into(),
            KeyFilter::PK(KfBy::Prefix("".into())),
            json!({
                "num_item": { "$lt": 5 },
                "array_item": [2, 3]
            })
            .try_into()
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0], doc);

    // == update ==

    let res = database_manager
        .update_item(
            database_id.clone(),
            TABLE_1_NAME.into(),
            KeyFilter::PK(KfBy::Exact("ex::1".into())),
            json!({
                "num_item": 1
            })
            .try_into()
            .unwrap(),
            json!({
                "$set": { "array_item.0": 10 },
                "$inc": { "array_item.1": 5 },
            })
            .try_into()
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    println!("update result: {:?}", res);

    // == insert one item into table 2 ==

    let pk = "ex::5";
    let sk = "0917";
    let doc_2 = json!({
        PK_ATTR: pk,
        SK_ATTR: sk,
        "array_item": ["h", "e", "l", "l", "o"],
        "obj_item": {
            "a": 10,
            "b": "hel",
        }
    })
    .to_string();
    let res = database_manager
        .insert_one_item(database_id.clone(), TABLE_2_NAME.into(), doc_2.clone())
        .await;
    assert_eq!(res, Ok(pk.to_string()));
    println!("Inserted key: {:?}", res);

    // == insert one item with already exsited secondary key into table 2 ==

    let pk = "ex::6";
    let sk = "0917";
    let doc_3 = json!({
        PK_ATTR: pk,
        SK_ATTR: sk,
        "array_item": ["x"],
    })
    .to_string();
    let res = database_manager
        .insert_one_item(database_id.clone(), TABLE_2_NAME.into(), doc_3.clone())
        .await;
    assert_eq!(
        res,
        Err(Error::SecondaryKeyAlreadyExist(SK_ATTR.into(), sk.into()))
    );
    println!("Inserted key: {:?}", res);

    // == concurrent db access (find/update) test ==

    let mut handles = vec![];
    (0..10).for_each(|_| {
        let dm_clone = database_manager.clone();
        let db_id_clone = database_id.clone();
        let handle = ::tokio::spawn(async move {
            find_and_update_again(&dm_clone, &db_id_clone, TABLE_1_NAME).await
        });
        handles.push(handle);
    });

    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // == delete item from table 2 ==

    let res = database_manager
        .delete_item(
            database_id.clone(),
            TABLE_2_NAME.into(),
            KeyFilter::PK(KfBy::Prefix("".into())),
            json!({
                "obj_item": { "a": 10 }
            })
            .try_into()
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0], doc_2);

    // == delete table 1 ==

    let res = database_manager
        .delete_table(database_id.clone(), TABLE_1_NAME.into())
        .await;
    assert_eq!(
        res,
        Ok(Some(TableDescription {
            table_name: TABLE_1_NAME.into(),
            indexes: indexes.clone()
        }))
    );

    // == delete table 2 ==

    let res = database_manager
        .delete_table(database_id.clone(), TABLE_2_NAME.into())
        .await;
    assert_eq!(
        res,
        Ok(Some(TableDescription {
            table_name: TABLE_2_NAME.into(),
            indexes: indexes_2.clone()
        }))
    );

    // == drop db ==
    database_manager.drop_db(&database_id).await.unwrap();
}
