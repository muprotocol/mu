use musdk::*;

#[mu_functions]
mod hello_wasm {
    use super::*;

    #[mu_function]
    fn say_hello<'a>(_ctx: &'a MuContext, _request: BinaryBody<'a>) -> BinaryResponse {
        panic!("Let me get out");
    }
}
