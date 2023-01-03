use musdk_derive::mu_functions;

// No actual tests here, just letting rustc compile the code generated
// by mu_functions to see if it compiles at all.

#[mu_functions]
mod functions {
    use musdk::{BinaryBody, BinaryResponse, MuContext};

    #[mu_function]
    fn simple_function<'a>(_ctx: &'a MuContext, request: BinaryBody<'a>) -> BinaryResponse {
        BinaryResponse::new(request.body.to_vec())
    }
}
