use super::{
    db::TableDescription,
    input::{Item, Key},
};
use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct CreateTableOutput {
    pub table_description: TableDescription,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct DeleteTableOutput {
    pub table_description: Option<TableDescription>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct InsertOneItemOutput {
    pub key: Key,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct FindItemOutput {
    pub items: Vec<Item>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct UpdateItemOutput {
    pub items: Vec<Item>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct DeleteItemOutput {
    pub keys: Vec<Key>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct DeleteAllItemsOutput;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct TableLenOutput {
    pub len: usize,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct TableIsEmptyOutput {
    pub is_empty: bool,
}
