use super::{
    client::*,
    error::Result,
    input::*,
    output::*,
    query::{Filter, Update},
};

pub async fn create_table(
    database_id: DatabaseID,
    table_name: String,
) -> Result<CreateTableOutput> {
    CreateTable {
        database_id,
        input: CreateTableInput { table_name },
    }
    .run()
    .await
}

pub async fn delete_table(
    database_id: DatabaseID,
    table_name: String,
) -> Result<DeleteTableOutput> {
    DeleteTable {
        database_id,
        input: DeleteTableInput { table_name },
    }
    .run()
    .await
}

pub async fn insert_one_item(
    database_id: DatabaseID,
    table_name: String,
    key: Key,
    value: Value,
) -> Result<InsertOneItemOutput> {
    InsertOneItem {
        database_id,
        input: InsertOneItemInput {
            table_name,
            key,
            value,
        },
    }
    .run()
    .await
}

pub async fn find_item(
    database_id: DatabaseID,
    table_name: String,
    key_filter: KeyFilter,
    filter: Option<Filter>,
) -> Result<FindItemOutput> {
    FindItem {
        database_id,
        input: FindItemInput {
            table_name,
            key_filter,
            filter,
        },
    }
    .run()
    .await
}

pub async fn update_item(
    database_id: DatabaseID,
    table_name: String,
    key_filter: KeyFilter,
    filter: Option<Filter>,
    update: Update,
) -> Result<UpdateItemOutput> {
    UpdateItem {
        database_id,
        input: UpdateItemInput {
            table_name,
            key_filter,
            filter,
            update,
        },
    }
    .run()
    .await
}

pub async fn delete_item(
    database_id: DatabaseID,
    table_name: String,
    key_filter: KeyFilter,
    filter: Option<Filter>,
) -> Result<DeleteItemOutput> {
    DeleteItem {
        database_id,
        input: DeleteItemInput {
            table_name,
            key_filter,
            filter,
        },
    }
    .run()
    .await
}

pub async fn delete_all_items(
    database_id: DatabaseID,
    table_name: String,
) -> Result<DeleteAllItemsOutput> {
    DeleteAllItems {
        database_id,
        input: DeleteAllItemsInput { table_name },
    }
    .run()
    .await
}

pub async fn table_len(database_id: DatabaseID, table_name: String) -> Result<TableLenOutput> {
    TableLen {
        database_id,
        input: TableLenInput { table_name },
    }
    .run()
    .await
}

pub async fn table_is_empty(
    database_id: DatabaseID,
    table_name: String,
) -> Result<TableIsEmptyOutput> {
    TableIsEmpty {
        database_id,
        input: TableIsEmptyInput { table_name },
    }
    .run()
    .await
}
