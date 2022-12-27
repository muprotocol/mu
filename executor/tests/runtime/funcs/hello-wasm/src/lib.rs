use musdk::*;

#[mu_functions]
mod hello_wasm {
    use super::*;

    #[mu_function]
    fn say_hello<'a>(_ctx: &'a MuContext, request: BinaryBody<'a>) -> BinaryResponse {
        let name = String::from_utf8_lossy(request.body);

        let response = format!("Hello {}, welcome to MuRuntime", name)
            .as_bytes()
            .to_vec();

        BinaryResponse::new(response)
    }
}
