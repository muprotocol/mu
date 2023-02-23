use musdk::*;

#[mu_functions]
mod functions {
    use musdk::{LogLevel, MuContext, PathParams};

    #[mu_function]
    fn greet_user_v2<'a>(ctx: &'a mut MuContext, data: &'a [u8]) -> Vec<u8> {
        let s = String::from_utf8_lossy(data);

        let mut count = ctx
            .db()
            .get("t1", data)
            .unwrap()
            .map(|v| v.0)
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_default();
        count = count.wrapping_add(1);
        ctx.db().put("t1", data, vec![count], false).unwrap();
        assert!(matches!(ctx.db().put("t2", "x", [0u8], false), Err(_)));
        ctx.db().put("t3", "x", [0u8], false).unwrap();

        let _ = ctx.log(&format!("Received request from {s}"), LogLevel::Info);
        format!("(#{count}) Hello from the second version, {s}!").into_bytes()
    }

    #[mu_function]
    fn greet_path_user_v2<'a>(ctx: &'a mut MuContext, path: PathParams<'a>) -> Vec<u8> {
        let name = path.get("name").expect("Expected to have name path param");

        let _ = ctx.log(&format!("Received request from {name}"), LogLevel::Info);
        format!("Hello from the second version, {name}!").into_bytes()
    }
}
