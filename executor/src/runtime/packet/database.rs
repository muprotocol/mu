use crate::{
    mudb::service::{DatabaseID, Item, Key, KeyFilter, TableDescription},
    runtime::types::FunctionID,
};

use anyhow::Result;
use borsh::{BorshDeserialize, BorshSerialize};

macro_rules! make_request {
    ($name:ident) => {
        #[derive(Debug, BorshDeserialize)]
        pub struct $name {
            pub db_name: String,
            pub table_name: String,
        }
    };

    ($name:ident, $($field:ident : $type:ty),*) => {
        #[derive(Debug, BorshDeserialize)]
        pub struct $name {
            pub db_name: String,
            pub table_name: String,
            $(
            pub $field: $type,
            )*
        }
    };
}

make_request!(CreateTableRequest);
make_request!(DropTableRequest);
make_request!(FindRequest, key_filter: KeyFilter, value_filter: String);
make_request!(InsertRequest, key: String, value: String);
make_request!(
    UpdateRequest,
    key_filter: KeyFilter,
    value_filter: String,
    update: String
);

#[derive(Debug, BorshDeserialize)]
pub enum Request {
    CreateTable(CreateTableRequest),
    DropTable(DropTableRequest),
    Find(FindRequest),
    Insert(InsertRequest),
    Update(UpdateRequest),
}

#[derive(Debug, BorshSerialize)]
pub enum Response {
    CreateTable(Result<TableDescription, String>),
    DropTable(Result<Option<TableDescription>, String>),
    Find(Result<Vec<Item>, String>),
    Insert(Result<Key, String>),
    Update(Result<Vec<Item>, String>),
}

pub fn database_id(function_id: &FunctionID, db_name: String) -> DatabaseID {
    DatabaseID {
        stack_id: function_id.stack_id,
        db_name,
    }
}
