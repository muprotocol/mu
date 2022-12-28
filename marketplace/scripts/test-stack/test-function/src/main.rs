use musdk::*;

#[mu_functions]
mod functions {
    use musdk::{BinaryBody, BinaryResponse, LogLevel, MuContext};

    #[mu_function]
    fn greet_user<'a>(ctx: &'a mut MuContext, request_body: BinaryBody<'a>) -> BinaryResponse {
        let s = String::from_utf8_lossy(request_body.body);
        let _ = ctx.log(&format!("Received request from {s}"), LogLevel::Info);
        BinaryResponse::new(format!("Hello, {s}!").into_bytes())
    }
}
