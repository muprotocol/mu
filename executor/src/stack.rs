use mu_stack::{Stack, StackID};
use solana_sdk::pubkey::Pubkey;

pub mod blockchain_monitor;
pub mod deploy;
pub mod scheduler;

#[derive(Clone, Debug)]
pub struct StackWithMetadata {
    stack: Stack,
    id: StackID,
    revision: u32,
    metadata: StackMetadata,
}

#[derive(Clone, Debug)]
pub enum StackMetadata {
    Solana(SolanaStackMetadata),
}

#[derive(Clone, Debug)]
pub struct SolanaStackMetadata {
    owner: Pubkey,
    seed: u64, // TODO: use uuid?
}
