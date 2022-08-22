use super::{FromMessage, Message, ToMessage};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Filter;

#[derive(Deserialize)]
pub enum DbRequestDetail {
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

// TODO: Change based on actual MuDB request types
pub struct DbRequest {
    id: u64,
    request: DbRequestDetail,
}

impl FromMessage for DbRequest {
    const TYPE: &'static str = "DbRequest";

    fn from_message(m: Message) -> Result<Self> {
        Ok(Self {
            id: m.id,
            request: serde_json::from_value(m.message)?,
        })
    }
}

#[derive(Serialize)]
pub enum DbResponseDetail {
    CreateTable(Result<(), &'static str>),
    DropTable(Result<(), &'static str>),
    Query(Result<Vec<(String, String)>, &'static str>),
    Insert(Result<(), &'static str>),
    InsertMany(Result<(), &'static str>),
}

#[derive(Serialize)]
pub struct DbResponse {
    id: u64,
    response: DbResponseDetail,
}

impl ToMessage for DbResponse {
    const TYPE: &'static str = "DbResponse";

    fn to_message(&self) -> Result<Message> {
        Ok(Message {
            id: self.id,
            r#type: Self::TYPE.to_owned(),
            message: serde_json::to_value(&self.response)?,
        })
    }
}
