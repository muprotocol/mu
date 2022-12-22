use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{stdin, stdout, Write},
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
    pub data: Vec<u8>,
}

#[derive(Serialize, Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct Log {
    body: String,
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
fn read_stdin() -> Message {
    let mut buf = String::new();
    loop {
        let bytes_read = stdin().read_line(&mut buf).unwrap();
        if bytes_read == 0 {
            continue;
        };

        let msg: Message = serde_json::from_str(&buf).unwrap();
        return msg;
    }
}

fn main() {
    let mut message_counter = 0;
    let mut log = |body| {
        message_counter += 1;
        let log = Log { body };
        send_message(log, "Log", Some(message_counter));
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

    let msg = read_stdin();

    let request: Request = serde_json::from_value(msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();

    let body = format!(
        "Hello {}, welcome to MuRuntime",
        String::from_utf8_lossy(&request.data)
    )
    .as_bytes()
    .to_vec();
    response(body);
}
