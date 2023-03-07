use musdk::*;

#[mu_functions]
mod functions {
    use musdk::{LogLevel, MuContext, PathParams};

    #[mu_function]
    fn greet_user<'a>(ctx: &'a mut MuContext, name: String) -> String {
        let mut count = ctx
            .db()
            .get("t1", &name)
            .unwrap()
            .map(|v| v.0)
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_default();
        count = count.wrapping_add(1);
        ctx.db().put("t1", &name, vec![count], false).unwrap();
        ctx.db().put("t2", "x", [0u8], false).unwrap();

        let _ = ctx.log(&format!("Received request from {name}"), LogLevel::Info);
        format!("(#{count}) Hello, {name}!")
    }

    #[mu_function]
    fn greet_path_user<'a>(ctx: &'a mut MuContext, path: PathParams<'a>) -> String {
        let name = path.get("name").expect("Expected to have name path param");

        let _ = ctx.log(&format!("Received request from {name}"), LogLevel::Info);
        format!("Hello, {name}!")
    }

    #[mu_function]
    fn long_greeting<'a>(_ctx: &'a MuContext, path: PathParams<'a>) -> String {
        let name = path.get("name").expect("Expected to have name path param");
        let mut count = 0;

        for i in 0..1_000_000_000u64 {
            if i.is_power_of_two() {
                count += 1;
            }
        }

        format!("Hello, {name}!, there is {count} powers of 2 in range of 0..1_000_000_000")
    }
}
