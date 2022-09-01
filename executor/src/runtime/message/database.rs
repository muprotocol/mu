use crate::{
    mudb::{
        client::DatabaseID,
        input::KeyFilter,
        output::{
            CreateTableOutput, DeleteTableOutput, FindItemOutput, InsertOneItemOutput,
            UpdateItemOutput,
        },
        query::Filter,
    },
    runtime::types::FunctionID,
};

use super::{FromMessage, Message, ToMessage};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! make_request {
    ($name:ident) => {
        #[derive(Deserialize)]
        pub struct $name {
            pub db_name: String,
            pub table_name: String,
        }
    };

    ($name:ident, $($field:ident : $type:ty),*) => {
        #[derive(Deserialize)]
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
make_request!(
    FindRequest,
    key_filter: KeyFilter,
    value_filter: Option<Filter>
);
make_request!(InsertRequest, key: String, value: String);
make_request!(
    UpdateRequest,
    key_filter: KeyFilter,
    value_filter: Option<Filter>,
    update: Value
);

#[derive(Deserialize)]
pub enum DbRequestDetails {
    CreateTable(CreateTableRequest),
    DropTable(DropTableRequest),
    Find(FindRequest),
    Insert(InsertRequest),
    Update(UpdateRequest),
}

pub struct DbRequest {
    pub id: u64,
    pub request: DbRequestDetails,
}

impl FromMessage for DbRequest {
    const TYPE: &'static str = "DbRequest";

    fn from_message(m: Message) -> Result<Self> {
        Ok(Self {
            id: m
                .id
                .context("filed id in database request can not be null")?,
            request: serde_json::from_value(m.message)
                .context("database request deserialization failed")?,
        })
    }
}

#[derive(Serialize)]
pub enum DbResponseDetails {
    CreateTable(Result<CreateTableOutput, String>),
    DropTable(Result<DeleteTableOutput, String>),
    Find(Result<FindItemOutput, String>),
    Insert(Result<InsertOneItemOutput, String>),
    Update(Result<UpdateItemOutput, String>),
}

#[derive(Serialize)]
pub struct DbResponse {
    pub id: u64,
    pub response: DbResponseDetails,
}

impl ToMessage for DbResponse {
    const TYPE: &'static str = "DbResponse";

    fn to_message(&self) -> Result<Message> {
        Ok(Message {
            id: Some(self.id),
            r#type: Self::TYPE.to_owned(),
            message: serde_json::to_value(&self.response)
                .context("database response serialization failed")?,
        })
    }
}

pub fn database_id(function_id: &FunctionID, database_name: String) -> DatabaseID {
    DatabaseID {
        stack_id: function_id.stack_id,
        database_name,
    }
}
