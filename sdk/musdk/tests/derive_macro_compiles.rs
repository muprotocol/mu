use musdk_derive::mu_functions;

// No actual tests here, just letting rustc compile the code generated
// by mu_functions to see if it compiles at all.

#[mu_functions]
mod functions {
    use musdk::MuContext;

    #[mu_function]
    fn simple_function<'a>(_ctx: &'a MuContext, data: &'a [u8]) -> Vec<u8> {
        data.to_vec()
    }
}
