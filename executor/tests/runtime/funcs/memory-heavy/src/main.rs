use musdk::*;

#[mu_functions]
mod hello_wasm {
    use super::*;

    #[mu_function]
    fn say_hello<'a>(_ctx: &'a MuContext, request: BinaryBody<'a>) -> BinaryResponse {
        let name = String::from_utf8_lossy(request.body);

        let mut huge_array = [0u8; 100_000_000];
        huge_array[87_654_321] = 145;

        let response = format!("Hello {}, welcome to MuRuntime", name)
            .as_bytes()
            .to_vec();

        BinaryResponse::new(response)
    }
}
