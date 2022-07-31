use std::io::{stdin, stdout, Write};

use serde::{Deserialize, Serialize};

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
    stdin().read_line(&mut buf).unwrap();
    let request: Request = serde_json::from_str(&buf).unwrap();

    let result = format!("Hello {}, welcome to MuRuntime", request.name);
    let resp = Response {
        req_id: request.req_id,
        result,
    };

    let resp = serde_json::to_string(&resp).unwrap();
    stdout().write_all(resp.as_bytes()).unwrap();
}
