#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::{
    collections::HashMap,
    io::{stdin, stdout, Write},
    sync::{Arc, RwLock},
};

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    id: Option<u32>,
    r#type: String,
    message: JsonValue,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Debug)]
pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: Vec<Header>,
    pub data: String,
}

#[derive(Serialize, Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<Header>,
    pub body: String,
}

#[derive(Debug, Serialize)]
struct Log {
    body: String,
}

#[derive(Debug, Serialize)]
enum DbRequest {
    CreateTable(CreateTableRequest),
    Insert(InsertRequest),
    Find(FindRequest),
}

#[derive(Debug, Serialize)]
pub struct Indexes {
    pub pk: String,
    pub sk: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CreateTableRequest {
    db_name: String,
    table_name: String,
    indexes: Indexes,
}

#[derive(Debug, Serialize)]
struct InsertRequest {
    db_name: String,
    table_name: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct FindRequest {
    db_name: String,
    table_name: String,
    key_filter: KeyFilter,
    value_filter: String,
}

// TODO: considraton KeyFilter<T>
#[derive(Debug, Serialize)]
pub enum KeyFilter {
    Exact(String),
    Prefix(String),
}

pub type Key = String;
pub type Value = String;

#[derive(Debug, Deserialize)]
pub enum DbResponse {
    CreateTable(Result<TableDescription, String>),
    Find(Result<Vec<Value>, String>),
    Insert(Result<Key, String>),
}

#[derive(Debug, Deserialize)]
pub struct TableDescription {
    pub table_name: String,
}

fn send_message<T: Serialize>(msg: T, msg_type: &str, id: Option<u32>) {
    let msg = Message {
        id,
        r#type: msg_type.into(),
        message: serde_json::to_value(msg).unwrap(),
    };

    let mut msg = serde_json::to_vec(&msg).unwrap();
    msg.push(b'\n');

    stdout().write_all(&msg).unwrap();
}

fn read_stdin(log: impl Fn(String)) -> Message {
    let mut buf = String::new();
    loop {
        let bytes_read = stdin()
            .read_line(&mut buf)
            .map_err(|e| log(e.to_string()))
            .unwrap();
        if bytes_read == 0 {
            std::thread::yield_now();
            continue;
        };

        let msg: Message = serde_json::from_str(&buf)
            .map_err(|e| log(e.to_string()))
            .unwrap();
        return msg;
    }
}

#[derive(Clone)]
struct Counter {
    inner: Arc<RwLock<u32>>,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(0)),
        }
    }

    pub fn get(&self) -> u32 {
        let mut inner = self.inner.write().unwrap();
        *inner += 1;
        *inner
    }
}

fn main() {
    let counter = Counter::new();

    let counter_clone = counter.clone();
    let log = move |body| {
        let log = Log { body };
        send_message(log, "Log", Some(counter_clone.get()));
    };

    let response = |body| {
        let resp = Response {
            status: 200,
            content_type: "plain".into(),
            headers: Vec::new(),
            body,
        };
        send_message(resp, "GatewayResponse", None);
    };

    let db_request = move |req| {
        let id = counter.get();
        send_message(req, "DbRequest", Some(id));
        id
    };

    let gateway_msg = read_stdin(&log);
    let request: Request = serde_json::from_value(gateway_msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    // Create table

    db_request(DbRequest::CreateTable(CreateTableRequest {
        db_name: "my_db".into(),
        table_name: "test_table".into(),
        indexes: Indexes {
            pk: "id".into(),
            sk: vec![],
        },
    }));

    let db_resp_msg = read_stdin(&log);

    let _: DbResponse = serde_json::from_value(db_resp_msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    let value = json!({
        "id": "secret",
        "x": "Mu Rocks!"
    })
    .to_string();

    // Insert data
    db_request(DbRequest::Insert(InsertRequest {
        db_name: "my_db".into(),
        table_name: "test_table".into(),
        value: value.clone(),
    }));

    let db_resp_msg = read_stdin(&log);
    let _: DbResponse = serde_json::from_value(db_resp_msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    // Find data
    db_request(DbRequest::Find(FindRequest {
        db_name: "my_db".into(),
        table_name: "test_table".into(),
        key_filter: KeyFilter::Exact("secret".into()),
        value_filter: json!({}).to_string(),
    }));

    let db_resp_msg = read_stdin(&log);
    let db_resp = serde_json::from_value::<DbResponse>(db_resp_msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    if let DbResponse::Find(db_resp) = db_resp {
        match db_resp {
            Err(e) => log(format!("Database Error: {e}")),
            Ok(r) if r[0] == value.clone() => (),
            Ok(r) => {
                log(format!("Find Error"));
                assert_eq!(r[0], value)
            }
        }
    }

    let body = format!("Hello {}", request.data);
    response(body);
}
