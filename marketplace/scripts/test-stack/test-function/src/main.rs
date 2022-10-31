#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashMap,
    io::{stdin, stdout, Write},
    rc::Rc,
};

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    id: Option<u32>,
    r#type: String,
    message: Value,
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
struct CreateTableRequest {
    db_name: String,
    table_name: String,
}

#[derive(Debug, Serialize)]
struct InsertRequest {
    db_name: String,
    table_name: String,
    key: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct FindRequest {
    db_name: String,
    table_name: String,
    key_filter: KeyFilter,
    value_filter: String,
}

#[derive(Debug, Serialize)]
pub enum KeyFilter {
    Exact(String),
    Prefix(String),
}

type Filter = serde_json::Value;
pub type Key = String;
pub type Item = (Key, String);

#[derive(Debug, Deserialize)]
pub enum DbResponse {
    CreateTable(Result<TableDescription, String>),
    Find(Result<Vec<Item>, String>),
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
    inner: Rc<RefCell<u32>>,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(0)),
        }
    }

    pub fn get_and_increment(&self) -> u32 {
        let mut inner = self.inner.borrow_mut();
        *inner += 1;
        *inner
    }
}

fn main() {
    let counter = Counter::new();

    let counter_clone = counter.clone();
    let log = move |body| {
        let log = Log { body };
        send_message(log, "Log", Some(counter_clone.get_and_increment()));
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
        let id = counter.get_and_increment();
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
        table_name: "visitors".into(),
    }));

    let db_resp_msg = read_stdin(&log);
    let _: DbResponse = serde_json::from_value(db_resp_msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    db_request(DbRequest::Find(FindRequest {
        db_name: "my_db".into(),
        table_name: "visitors".into(),
        key_filter: KeyFilter::Exact("count".into()),
        value_filter: json!({}).to_string(),
    }));

    // Note: don't do this, doesn't support concurrent requests, will mess up the counter under load
    let db_resp_msg = read_stdin(&log);
    let find_response: DbResponse = serde_json::from_value(db_resp_msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    let mut current: u64 = if let DbResponse::Find(Ok(out)) = find_response {
        if out.is_empty() {
            0u64
        } else {
            serde_json::from_str(&out[0].1).unwrap()
        }
    } else {
        panic!("Unexpected DB output: {:?}", find_response);
    };

    current += 1;

    // Insert data
    // Note, this doesn't work right now because inserts don't overwrite values
    db_request(DbRequest::Insert(InsertRequest {
        db_name: "my_db".into(),
        table_name: "visitors".into(),
        key: "count".into(),
        value: json!(current).to_string(),
    }));

    let db_resp_msg = read_stdin(&log);
    let _: DbResponse = serde_json::from_value(db_resp_msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    let body = format!(
        "Hello, {}! You are visitor number {}",
        request.data, current
    );
    response(body);
}
