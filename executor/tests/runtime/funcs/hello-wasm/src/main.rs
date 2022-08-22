use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::io::{stdin, stdout, Write};

#[derive(Deserialize, Serialize)]
struct Message {
    id: u32,
    r#type: String,
    message: Value,
}

#[derive(Deserialize)]
struct Request {
    req_id: u32,
    name: String,
}

#[derive(Serialize)]
struct Response {
    req_id: u32,
    result: String,
}

fn main() {
    let mut buf = String::new();

    loop {
        buf.clear();
        stdin().read_line(&mut buf).unwrap();

        let msg: Message = serde_json::from_str(&buf).unwrap();
        let request: Request = serde_json::from_value(msg.message).unwrap();

        let result = format!("Hello {}, welcome to MuRuntime", request.name);
        let resp = Response {
            req_id: request.req_id,
            result,
        };

        let resp = serde_json::to_value(&resp).unwrap();
        let msg = Message {
            message: resp,
            r#type: "GatewayResponse".to_owned(),
            id: msg.id,
        };
        let mut msg = serde_json::to_vec(&msg).unwrap();
        msg.push(b'\n');

        stdout().write_all(&msg).unwrap();
    }
}
