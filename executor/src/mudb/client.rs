use serde::Deserialize;

use super::{input::*, output::*, Config, MuDB, Result};

/// # Usage
///
/// ```ignore
/// make_type_from!(CreateTable, CreateTableInput);
/// ```
///
/// will generate:
///
/// ```ignore
/// #[derive(Debug, Deserialize)]
/// pub struct CreateTable {
///     pub conn_conf: Config,
///     pub input: CreateTableInput,
/// }
///
/// impl CreateTable {
///     async fn partial_run(
///         self,
///         run: impl Fn(&CreateTableInput, MuDB) -> Result<CreateTableOutput> + Send + Sync + 'static,
///     ) -> Result<CreateTableOutput> {
///         ::tokio::task::spawn_blocking(move || {
///             let db = MuDB::open_db(self.conn_conf)?;
///             run(&self.input, db)
///         })
///         .await?
///     }
/// }
/// ```
macro_rules! make_type_from {
    ($type_name:ident, $input_type:ident, $output_type:ident) => {
        #[derive(Debug, Deserialize)]
        pub struct $type_name {
            pub conn_conf: Config,
            pub input: $input_type,
        }

        impl $type_name {
            async fn partial_run(
                self,
                run: impl Fn($input_type, MuDB) -> Result<$output_type> + Send + Sync + 'static,
            ) -> Result<$output_type> {
                ::tokio::task::spawn_blocking(move || {
                    let db = MuDB::open_db(self.conn_conf)?;
                    run(self.input, db)
                })
                .await?
            }
        }
    };
}

make_type_from!(CreateTable, CreateTableInput, CreateTableOutput);
make_type_from!(DeleteTable, DeleteTableInput, DeleteTableOutput);
make_type_from!(InsertOneItem, InsertOneItemInput, InsertOneItemOutput);
// TODO
// make_type_from!(InsertMany, InsertManyInput);
make_type_from!(FindItem, FindItemInput, FindItemOutput);
make_type_from!(UpdateItem, UpdateItemInput, UpdateItemOutput);
make_type_from!(DeleteItem, DeleteItemInput, DeleteItemOutput);
make_type_from!(DeleteAllItems, DeleteAllItemInput, DeleteAllItemsOutput);
make_type_from!(TableLen, TableLenInput, TableLenOutput);
make_type_from!(TableIsEmpty, TableIsEmptyInput, TableIsEmptyOutput);

impl CreateTable {
    pub async fn run(self) -> Result<CreateTableOutput> {
        self.partial_run(|input, db| db.create_table(input)).await
    }
}

impl DeleteTable {
    pub async fn run(self) -> Result<DeleteTableOutput> {
        self.partial_run(|input, db| db.delete_table(input)).await
    }
}

impl InsertOneItem {
    pub async fn run(self) -> Result<InsertOneItemOutput> {
        self.partial_run(|input, db| db.insert_one_item(input))
            .await
    }
}

// TODO
// impl InsertMany {
//     pub fn run(self) -> Result<InsertManyOutput> {
//         self.partial_run(|input, db| db.insert_many(input))
//     }
// }

impl FindItem {
    pub async fn run(self) -> Result<FindItemOutput> {
        self.partial_run(|input, db| db.find_item(input)).await
    }
}

impl UpdateItem {
    pub async fn run(self) -> Result<UpdateItemOutput> {
        self.partial_run(|input, db| db.update_item(input)).await
    }
}

impl DeleteItem {
    pub async fn run(self) -> Result<DeleteItemOutput> {
        self.partial_run(|input, db| db.delete_item(input)).await
    }
}

impl DeleteAllItems {
    pub async fn run(self) -> Result<DeleteAllItemsOutput> {
        self.partial_run(|input, db| db.delete_all_items(input))
            .await
    }
}

impl TableLen {
    pub async fn run(self) -> Result<TableLenOutput> {
        self.partial_run(|input, db| db.table_len(input)).await
    }
}

impl TableIsEmpty {
    pub async fn run(self) -> Result<TableIsEmptyOutput> {
        self.partial_run(|input, db| db.table_is_empty(input)).await
    }
}
