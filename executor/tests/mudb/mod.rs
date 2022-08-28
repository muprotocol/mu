use mu::mudb::{client, db::TableDescription, input::*, output::*, query, Config, MuDB};
use serde_json::json;
use serial_test::serial;

#[test]
#[serial]
fn test_mudb_operations() {
    // init db
    let conf = Config {
        name: "test_mudb".to_owned(),
        ..Default::default()
    };

    let db = MuDB::open_db(conf).unwrap();

    // create table
    let table_1 = CreateTableInput {
        table_name: "table_1".to_string(),
    };
    let table_2_auto_key = CreateTableInput {
        table_name: "table_2_auto_key".to_string(),
    };
    db.create_table(table_1).unwrap();
    db.create_table(table_2_auto_key).unwrap();

    // insert one items
    let value1 = json!({
        "num_item": 1,
        "array_item": [1, 2, 3, 4],
        "obj_item": {
            "in_1": "hello",
            "in_2": "world",
        }
    })
    .to_string();
    let input1 = InsertOneItemInput {
        table_name: "table_1".to_string(),
        key: "ex::1".to_string(),
        value: value1.clone(),
    };
    let res1 = db.insert_one_item(input1).unwrap();
    assert_eq!(
        res1,
        InsertOneItemOutput {
            key: "ex::1".to_string()
        }
    );

    // insert one items
    let input2 = InsertOneItemInput {
        table_name: "table_2_auto_key".to_string(),
        key: "ex::5".to_string(),
        value: json!({
            "array_item": ["h", "e", "l", "l", "o"],
            "obj_item": {
                "a": 10,
                "b": "hel",
            }
        })
        .to_string(),
    };
    let insert_one_res = db.insert_one_item(input2).unwrap();
    dbg!(&insert_one_res);
    println!("Inserted key: {:?}", insert_one_res);

    // TODO
    // // get table names
    // assert_eq!(
    //     db._table_names(),
    //     Ok(vec!["table_1".to_string(), "table_2_auto_key".to_string()])
    // );

    // find
    let input3 = FindItemInput {
        table_name: "table_1".to_string(),
        key_filter: KeyFilter::Prefix("".to_string()),
        filter: Some(query::Filter(json!({
            "num_item": { "$lt": 5 },
            "array_item": [2, 3]
        }))),
    };
    let find_res = db.find_item(input3).unwrap();
    dbg!(&find_res);
    assert_eq!(find_res.items[0].0, "ex::1".to_owned());
    assert_eq!(find_res.items[0].1, value1);

    // update
    let input = UpdateItemInput {
        table_name: "table_1".to_string(),
        key_filter: KeyFilter::Exact("ex::1".to_string()),
        filter: Some(query::Filter(json!({
            "num_item": 1
        }))),
        update: query::Update(json!({
            "$set": { "array_item.0": 10 },
            "$inc": { "array_item.1": 5 },
        })),
    };
    let update_res = db.update_item(input).unwrap();
    dbg!(&update_res);
    assert_eq!(update_res.items.len(), 1);

    // delete
    let input = DeleteItemInput {
        table_name: "table_2_auto_key".to_string(),
        key_filter: KeyFilter::Prefix("".to_string()),
        filter: Some(query::Filter(json!({
            "obj_item": { "a": 10 }
        }))),
    };

    let del_res = db.delete_item(input).unwrap();
    dbg!(&del_res);
    assert_eq!(del_res.keys.len(), 1);

    // delete table 1
    let input = DeleteTableInput {
        table_name: "table_1".to_string(),
    };
    let res = db.delete_table(input);

    assert_eq!(
        res,
        Ok(DeleteTableOutput {
            table_description: Some(TableDescription {
                table_name: "table_1".to_string(),
            })
        })
    );

    // delete table 2
    let input = DeleteTableInput {
        table_name: "table_2_auto_key".to_string(),
    };
    let res = db.delete_table(input);

    assert_eq!(
        res,
        Ok(DeleteTableOutput {
            table_description: Some(TableDescription {
                table_name: "table_2_auto_key".to_string(),
            })
        })
    );
}

#[tokio::test]
#[serial]
async fn test_mudb_stateless_api() {
    // // init db
    // let conf = Config {
    //     name: "test_mudb".to_owned(),
    //     ..Default::default()
    // };

    let database_id = client::DatabaseID {
        stack_id: mu::mu_stack::StackID(uuid::Uuid::new_v4()),
        database_name: "test_mudb".to_owned(),
    };

    // create table 1
    let res = client::CreateTable {
        database_id: database_id.clone(),
        input: CreateTableInput {
            table_name: "table_1".to_string(),
        },
    }
    .run()
    .await;

    assert_eq!(
        res,
        Ok(CreateTableOutput {
            table_description: mu::mudb::db::TableDescription {
                table_name: "table_1".to_string(),
            }
        })
    );

    // create table 2
    let res = client::CreateTable {
        database_id: database_id.clone(),
        input: CreateTableInput {
            table_name: "table_2_auto_key".to_string(),
        },
    }
    .run()
    .await;

    assert_eq!(
        res,
        Ok(CreateTableOutput {
            table_description: mu::mudb::db::TableDescription {
                table_name: "table_2_auto_key".to_string(),
            }
        })
    );

    // insert one items
    let insert_one_input_value = json!({
        "num_item": 1,
        "array_item": [1, 2, 3, 4],
        "obj_item": {
            "in_1": "hello",
            "in_2": "world",
        }
    })
    .to_string();

    let res = client::InsertOneItem {
        database_id: database_id.clone(),
        input: InsertOneItemInput {
            table_name: "table_1".to_string(),
            key: "ex::1".to_string(),
            value: insert_one_input_value.clone(),
        },
    }
    .run()
    .await;

    assert_eq!(
        res,
        Ok(InsertOneItemOutput {
            key: "ex::1".to_string()
        })
    );

    // insert one items
    let insert_one_res = client::InsertOneItem {
        database_id: database_id.clone(),
        input: InsertOneItemInput {
            table_name: "table_2_auto_key".to_string(),
            key: "ex::5".to_string(),
            value: json!({
                "array_item": ["h", "e", "l", "l", "o"],
                "obj_item": {
                    "a": 10,
                    "b": "hel",
                }
            })
            .to_string(),
        },
    }
    .run()
    .await
    .unwrap();

    dbg!(&insert_one_res);
    println!("Inserted key: {:?}", insert_one_res);

    // find
    let find_res = client::FindItem {
        database_id: database_id.clone(),
        input: FindItemInput {
            table_name: "table_1".to_string(),
            key_filter: KeyFilter::Prefix("".to_string()),
            filter: Some(query::Filter(json!({
                "num_item": { "$lt": 5 },
                "array_item": [2, 3]
            }))),
        },
    }
    .run()
    .await
    .unwrap();

    dbg!(&find_res);
    assert_eq!(find_res.items[0].0, "ex::1".to_string());
    assert_eq!(find_res.items[0].1, insert_one_input_value);

    // update
    let update_res = client::UpdateItem {
        database_id: database_id.clone(),
        input: UpdateItemInput {
            table_name: "table_1".to_string(),
            key_filter: KeyFilter::Exact("ex::1".to_string()),
            filter: Some(query::Filter(json!({
                "num_item": 1
            }))),
            update: query::Update(json!({
                "$set": { "array_item.0": 10 },
                "$inc": { "array_item.1": 5 },
            })),
        },
    }
    .run()
    .await
    .unwrap();

    dbg!(&update_res);
    assert_eq!(update_res.items.len(), 1);

    // delete
    let del_res = client::DeleteItem {
        database_id: database_id.clone(),
        input: DeleteItemInput {
            table_name: "table_2_auto_key".to_string(),
            key_filter: KeyFilter::Prefix("".to_string()),
            filter: Some(query::Filter(json!({
                "obj_item": { "a": 10 }
            }))),
        },
    }
    .run()
    .await
    .unwrap();

    dbg!(&del_res);
    assert_eq!(del_res.keys.len(), 1);

    // delete table 1
    let res = client::DeleteTable {
        database_id: database_id.clone(),
        input: DeleteTableInput {
            table_name: "table_1".to_string(),
        },
    }
    .run()
    .await;

    assert_eq!(
        res,
        Ok(DeleteTableOutput {
            table_description: Some(TableDescription {
                table_name: "table_1".to_string(),
            })
        })
    );

    // delete table 2
    let res = client::DeleteTable {
        database_id: database_id.clone(),
        input: DeleteTableInput {
            table_name: "table_2_auto_key".to_string(),
        },
    }
    .run()
    .await;

    assert_eq!(
        res,
        Ok(DeleteTableOutput {
            table_description: Some(TableDescription {
                table_name: "table_2_auto_key".to_string(),
            })
        })
    );
}
