use super::{input::*, output::*, Config, MuDB, Result};
use crate::mu_stack::StackID;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseID {
    pub stack_id: StackID,
    pub database_name: String,
}

impl DatabaseID {
    pub fn database_name(stack_id: StackID, name: &String) -> String {
        format!("{}_{}", stack_id, name.replace(" ", "-"))
    }
}

impl ToString for DatabaseID {
    fn to_string(&self) -> String {
        DatabaseID::database_name(self.stack_id, &self.database_name)
    }
}

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
///     pub database_name: String,
///     pub input: CreateTableInput,
/// }
///
/// impl CreateTable {
///     async fn partial_run(
///         self,
///         run: impl Fn(&CreateTableInput, MuDB) -> Result<CreateTableOutput> + Send + Sync + 'static,
///     ) -> Result<CreateTableOutput> {
///         ::tokio::task::spawn_blocking(move || {
///             let conf = Config {
///                 name: self.database_name,
///                 ..Default::default()
///             };
///
///             let db = MuDB::open_db(conf)?;
///             run(self.input, db)
///         })
///         .await?
///     }
/// }
/// ```
macro_rules! make_type_from {
    ($type_name:ident, $input_type:ident, $output_type:ident) => {
        #[derive(Debug)]
        pub struct $type_name {
            pub database_id: DatabaseID,
            pub input: $input_type,
        }

        impl $type_name {
            async fn partial_run(
                self,
                run: impl Fn($input_type, MuDB) -> Result<$output_type> + Send + Sync + 'static,
            ) -> Result<$output_type> {
                ::tokio::task::spawn_blocking(move || {
                    let database_id = format!(
                        "{}_{}",
                        self.database_id.stack_id,
                        self.database_id.database_name.replace(" ", "-")
                    );
                    // TODO: that's prototype
                    let conf = Config {
                        name: database_id,
                        ..Default::default()
                    };

                    let db = MuDB::open_db(conf)?;
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
make_type_from!(DeleteAllItems, DeleteAllItemsInput, DeleteAllItemsOutput);
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
