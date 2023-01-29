use musdk::*;

#[mu_functions]
mod calc_func {
    use super::*;

    #[mu_function]
    fn add_one<'a>(_ctx: &'a MuContext, number: &'a [u8]) -> Vec<u8> {
        let mut num = u32::from_be_bytes([number[0], number[1], number[2], number[3]]);
        num += 2;
        num.to_be_bytes().to_vec()
    }
}
