use serde::{Deserialize, Serialize};
use spl_token::solana_program::{hash::Hash, pubkey::Pubkey};

#[derive(Deserialize)]
pub struct AirdropRequest {
    amount: u64,
    to: Pubkey,
    blockhash: Hash,
}

#[derive(Serialize)]
pub struct AirdropResponse {
    signature: String, //TODO: use Signature
}

#[derive(Serialize)]
pub enum Error {}
