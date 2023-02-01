use musdk::*;

#[mu_functions]
mod functions {
    use musdk::{LogLevel, MuContext, PathParams};

    #[mu_function]
    fn greet_user_v2<'a>(ctx: &'a mut MuContext, data: &'a [u8]) -> Vec<u8> {
        let s = String::from_utf8_lossy(data);
        let _ = ctx.log(&format!("Received request from {s}"), LogLevel::Info);
        format!("Hello from the second version, {s}!").into_bytes()
    }

    #[mu_function]
    fn greet_path_user_v2<'a>(ctx: &'a mut MuContext, path: PathParams<'a>) -> Vec<u8> {
        let name = path.get("name").expect("Expected to have name path param");

        let _ = ctx.log(&format!("Received request from {name}"), LogLevel::Info);
        format!("Hello from the second version, {name}!").into_bytes()
    }
}
