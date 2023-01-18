use musdk::*;

#[mu_functions]
mod functions {
    use musdk::{LogLevel, MuContext, PathParams, Status};

    #[mu_function]
    fn greet_user<'a>(ctx: &'a mut MuContext, data: &'a [u8]) -> Vec<u8> {
        let s = String::from_utf8_lossy(data);
        let _ = ctx.log(&format!("Received request from {s}"), LogLevel::Info);
        format!("Hello, {s}!").into_bytes()
    }

    #[mu_function]
    fn greet_path_user<'a>(
        ctx: &'a mut MuContext,
        path: PathParams<'a>,
    ) -> Result<Vec<u8>, (&'static str, Status)> {
        let Some(name) = path.get("name") else {
            return Err(("Name not provided in path", Status::BadRequest));
        };

        let _ = ctx.log(&format!("Received request from {name}"), LogLevel::Info);
        Ok(format!("Hello, {name}!").into_bytes())
    }
}
