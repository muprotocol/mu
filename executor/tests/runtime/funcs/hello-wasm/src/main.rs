use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{stdin, stdout, Write},
    sync::Mutex,
};

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    id: u32,
    r#type: String,
    message: Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Request {
    local_path_and_query: String,
    body: String,
}

#[derive(Debug, Serialize)]
struct Response {
    body: String,
}

#[derive(Debug, Serialize)]
struct Log {
    body: String,
}

fn send_message<T: Serialize>(msg: T, msg_type: &str, counter: &mut u32, id: Option<u32>) {
    let id = id.unwrap_or_else(|| {
        *counter += 1;
        *counter
    });

    let msg = Message {
        id,
        r#type: msg_type.into(),
        message: serde_json::to_value(msg).unwrap(),
    };

    let mut msg = serde_json::to_vec(&msg).unwrap();
    msg.push(b'\n');

    stdout().write_all(&msg).unwrap();
}
fn read_stdin(mut log: impl FnMut(String)) -> Message {
    let mut buf = String::new();
    loop {
        let bytes_read = stdin()
            .read_line(&mut buf)
            .map_err(|e| log(e.to_string()))
            .unwrap();
        if bytes_read == 0 {
            continue;
        };

        let msg: Message = serde_json::from_str(&buf)
            .map_err(|e| log(e.to_string()))
            .unwrap();
        log(format!("[function]: Got message: {msg:?}"));
        return msg;
    }
}

fn main() {
    let message_counter = Mutex::new(0);
    let log = |body| {
        let log = Log { body };
        send_message(log, "Log", &mut message_counter.lock().unwrap(), None);
    };

    let response = |id, body| {
        let resp = Response { body };
        send_message(
            resp,
            "GatewayResponse",
            &mut message_counter.lock().unwrap(),
            Some(id),
        );
    };

    let msg = read_stdin(log);

    let request: Request = serde_json::from_value(msg.message)
        .map_err(|e| log(e.to_string()))
        .unwrap();
    log(format!(
        "[function]: made request out of message: {request:?}"
    ));

    let body = format!("Hello {}, welcome to MuRuntime", request.body);
    response(msg.id, body);
}
