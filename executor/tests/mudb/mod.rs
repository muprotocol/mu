use mu::mudb::{service::*, Config, Result};
use serde_json::json;
use serial_test::serial;

async fn find_and_update_again(
    db_service: &DatabaseManager,
    database_id: &DatabaseID,
    table_1: &str,
) -> Result<()> {
    // find
    db_service
        .find_item(
            database_id.clone(),
            table_1.into(),
            KeyFilter::Prefix("".into()),
            json!({
                "num_item": { "$lt": 5 },
                "array_item": [2, 3]
            })
            .try_into()
            .unwrap(),
        )
        .await?;

    // update
    db_service
        .update_item(
            database_id.clone(),
            table_1.into(),
            KeyFilter::Exact("ex::1".into()),
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

#[tokio::test]
#[serial]
async fn test_mudb_service() {
    let db_service = DatabaseManager::new().await.unwrap();

    // init db

    let database_id = DatabaseID {
        db_name: "test_mudb_service".into(),
        ..Default::default()
    };
    let conf = Config {
        database_id: database_id.clone(),
        ..Default::default()
    };
    db_service.create_db(conf).await.unwrap();

    // create table 1

    let table_1 = "table_1";
    let indexes = Indexes {
        primary_key: PK_ATTR.into(),
    };
    db_service
        .create_table(database_id.clone(), table_1.into(), indexes.clone())
        .await
        .unwrap();

    // create table 2

    let table_2 = "table_2";
    db_service
        .create_table(database_id.clone(), table_2.into(), indexes.clone())
        .await
        .unwrap();

    // insert one item

    let value1 = json!({
        PK_ATTR: "ex::1",
        "num_item": 1,
        "array_item": [1, 2, 3, 4],
        "obj_item": {
            "in_1": "hello",
            "in_2": "world",
        }
    })
    .to_string();

    let res1 = db_service
        .insert_one_item(database_id.clone(), table_1.into(), value1.clone())
        .await;

    assert_eq!(res1, Ok("ex::1".to_string()));

    // insert one item

    let insert_one_res = db_service
        .insert_one_item(
            database_id.clone(),
            table_2.into(),
            json!({
                PK_ATTR: "ex::5",
                "array_item": ["h", "e", "l", "l", "o"],
                "obj_item": {
                    "a": 10,
                    "b": "hel",
                }
            })
            .to_string(),
        )
        .await;

    dbg!(&insert_one_res);
    println!("Inserted key: {:?}", insert_one_res);

    // TODO
    // // get table names
    // assert_eq!(
    //     db._table_names(),
    //     Ok(vec!["table_1".to_string(), "table_2_auto_key".to_string()])
    // );

    // find

    let find_res = db_service
        .find_item(
            database_id.clone(),
            table_1.into(),
            KeyFilter::Prefix("".into()),
            json!({
                "num_item": { "$lt": 5 },
                "array_item": [2, 3]
            })
            .try_into()
            .unwrap(),
        )
        .await
        .unwrap();

    dbg!(&find_res);
    assert_eq!(find_res[0].0, "ex::1".to_string());
    assert_eq!(find_res[0].1, value1);

    // update

    let update_res = db_service
        .update_item(
            database_id.clone(),
            table_1.into(),
            KeyFilter::Exact("ex::1".into()),
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
        .await;

    dbg!(&update_res);
    assert_eq!(update_res.unwrap().len(), 1);

    // concurrent db access (find/update) test

    let mut handles = vec![];
    (0..10).for_each(|_| {
        let db_service_clone = db_service.clone();
        let db_id_clone = database_id.clone();
        let handle = ::tokio::spawn(async move {
            find_and_update_again(&db_service_clone, &db_id_clone, table_1).await
        });
        handles.push(handle);
    });

    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // delete

    let del_res = db_service
        .delete_item(
            database_id.clone(),
            table_2.into(),
            KeyFilter::Prefix("".into()),
            json!({
                "obj_item": { "a": 10 }
            })
            .try_into()
            .unwrap(),
        )
        .await;

    dbg!(&del_res);
    assert_eq!(del_res.unwrap().len(), 1);

    // delete table 1

    let res = db_service
        .delete_table(database_id.clone(), table_1.into())
        .await;

    assert_eq!(
        res,
        Ok(Some(TableDescription {
            table_name: "table_1".into(),
            indexes: indexes.clone()
        }))
    );

    // delete table 2

    let res = db_service
        .delete_table(database_id.clone(), table_2.into())
        .await;

    assert_eq!(
        res,
        Ok(Some(TableDescription {
            table_name: "table_2".into(),
            indexes: indexes.clone()
        }))
    );

    // drop db
    db_service.drop_db(&database_id).await.unwrap();
}
