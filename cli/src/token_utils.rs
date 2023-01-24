use spl_token::state::Mint;

pub fn ui_amount_to_token_amount(mint: &Mint, amount: f64) -> u64 {
    (amount * 10u64.pow(mint.decimals as u32) as f64).round() as u64
}

pub fn token_amount_to_ui_amount(mint: &Mint, amount: u64) -> f64 {
    (amount as f64) / 10f64.powi(mint.decimals as i32)
}
