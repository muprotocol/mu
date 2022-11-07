use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{stdin, stdout, Write};

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    id: Option<u32>,
    r#type: String,
    message: Value,
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

    let _msg = read_stdin();
    log("Eary Exit!".into());
}
