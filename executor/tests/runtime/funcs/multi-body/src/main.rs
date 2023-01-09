use musdk::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Form {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct Response {
    pub token: String,
    pub ttl: u64,
}

#[mu_functions]
mod hello_wasm {
    use super::*;

    #[mu_function]
    fn json_body<'a>(_ctx: &'a MuContext, request: Json<Form>) -> Json<Response> {
        let request = request.into_inner();
        Json(Response {
            token: format!("token_for_{}_{}", request.username, request.password),
            ttl: 14011018,
        })
    }

    #[mu_function]
    fn string_body<'a>(_ctx: &'a MuContext, request: String) -> String {
        format!("Hello {request}, got your message")
    }
}
