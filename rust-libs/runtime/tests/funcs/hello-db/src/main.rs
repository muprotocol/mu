use std::{
    borrow::Cow,
    collections::HashMap,
    io::{stdout, Write},
};

use musdk::*;
use musdk_common::{
    incoming_message::{db::ListResult, ExecuteFunction, IncomingMessage},
    outgoing_message::OutgoingMessage,
};
use serde::{Deserialize, Serialize};

// fn main() {
//     let request = Request {
//         method: HttpMethod::Get,
//         path_params: HashMap::new(),
//         query_params: HashMap::new(),
//         headers: vec![],
//         body: Cow::Borrowed(&[]),
//     };

//     let request = IncomingMessage::ExecuteFunction(ExecuteFunction {
//         function: Cow::Borrowed("create"),
//         request,
//     });

//     request.write(&mut stdout()).unwrap();

//     let resp = IncomingMessage::ListResult(ListResult {
//         items: vec![Cow::Borrowed(b"table_xxx")],
//     });

//     resp.write(&mut stdout()).unwrap();
// }

#[derive(Deserialize, Serialize, Debug)]
pub struct Create {
    pub table_name: String,
    pub key: String,
    pub value: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Read {
    pub table_name: String,
    pub key: String,
}

#[mu_functions]
mod hello_db {
    use super::*;

    #[mu_function]
    fn failed_on_getting_non_exist_table<'a>(ctx: &'a mut MuContext) {
        ctx.db().table("xxx").unwrap();
    }

    #[mu_function]
    fn create<'a>(ctx: &'a mut MuContext) {
        ctx.log("started!", LogLevel::Debug).unwrap();
        // let req = req.into_inner();
        let is_atomic = false;
        let mut x = ctx.db();
        let mut y = x.table_list("a::").unwrap();
        ctx.log(format!("{}", y.len()).as_str(), LogLevel::Info)
            .unwrap();
        // let mut x = x.table(&req.table_name);
        // ctx.db()
        //     .table(&req.table_name)
        //     .unwrap()
        //     .put(req.key.as_bytes(), req.value.as_bytes(), is_atomic)
        //     .unwrap();
        ctx.log("done!", LogLevel::Debug).unwrap();
    }

    #[mu_function]
    fn read<'a>(ctx: &'a mut MuContext, req: Json<Read>) -> String {
        let req = req.into_inner();
        ctx.db()
            .table(&req.table_name)
            .unwrap()
            .get(req.key.as_bytes())
            .unwrap()
            .map(|x| String::from_utf8_lossy(x.as_ref()).into_owned())
            .unwrap_or("".into())
    }

    #[mu_function]
    fn delete<'a>(ctx: &'a mut MuContext, req: Json<Read>) {
        let req = req.into_inner();
        let is_atomic = false;
        ctx.db()
            .table(&req.table_name)
            .unwrap()
            .delete(req.key.as_bytes(), is_atomic)
            .unwrap()
    }

    // #[mu_function]
    // fn scan<'a>(ctx: &'a mut MuContext, table_name: String, key_prefix: String) -> Vec<String> {
    //     let limit = 15;
    //     let blob_to_string = |x| String::from_utf8_lossy(x.as_ref()).into_owned();
    //     ctx.db()
    //         .table(&table_name)
    //         .unwrap()
    //         .scan(key_prefix.as_bytes(), limit)
    //         .unwrap()
    //         .map(|(k, v)| (blob_to_string(k), blob_to_string(v)))
    //         .unwrap_or("".into())
    // }
}
