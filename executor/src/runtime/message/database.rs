use serde::{Deserialize, Serialize};

use super::message::Input;

#[derive(Deserialize)]
pub struct Filter;

#[derive(Deserialize)]
pub enum DbRequest {
    CreateTable {
        table_name: String,
        auto_increment_id: bool,
    },
    DropTable {
        table_name: String,
    },
    Query {
        table_name: String,
        filter: Filter,
    },
    Insert {
        table_name: String,
        key: String,
        value: String,
    },
    InsertMany {
        table_name: String,
        items: Vec<(String, String)>,
    },
}

#[allow(dead_code)]
#[derive(Serialize)]
pub enum DbResponse {
    CreateTable(Result<(), &'static str>),
    DropTable(Result<(), &'static str>),
    Query(Result<Vec<(String, String)>, &'static str>),
    Insert(Result<(), &'static str>),
    InsertMany(Result<(), &'static str>),
}

impl Input for DbResponse {}
