use itertools::Itertools;
use musdk::*;

#[mu_functions]
mod hello_wasm {
    use super::*;

    #[mu_function]
    fn say_hello<'a>(ctx: &'a mut MuContext, name: &'a str) -> String {
        ctx.log("say_hello_started!", LogLevel::Debug).unwrap();
        format!("Hello {}, welcome to MuRuntime", name)
    }

    #[mu_function]
    fn memory_heavy<'a>(_ctx: &'a MuContext, data: String) -> String {
        let mut huge_array = [0u8; 100_000_000];
        huge_array[87_654_321] = 145;
        data
    }

    #[mu_function]
    fn failing<'a>(_ctx: &'a MuContext) {
        panic!("Let me get out of here!");
    }

    #[mu_function]
    fn path_params<'a>(_ctx: &'a MuContext, req: &'a Request<'a>) -> String {
        req.path_params
            .iter()
            .sorted_by(|i, j| i.0.cmp(j.0))
            .map(|(k, v)| format!("{k}:{v}"))
            .reduce(|i, j| format!("{i},{j}"))
            .unwrap_or("".into())
    }

    #[mu_function]
    fn long_running<'a>(ctx: &'a mut MuContext) -> String {
        for i in 0..1_000_000_000u64 {
            if i.is_power_of_two() {
                ctx.log(format!("{i} is power of 2"), LogLevel::Info)
                    .unwrap();
            }
        }
        "Hey!".into()
    }
}
