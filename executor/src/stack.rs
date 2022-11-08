use mu_stack::{Stack, StackID};
use solana_sdk::pubkey::Pubkey;

pub mod blockchain_monitor;
mod config_types;
pub mod deploy;
pub mod scheduler;
pub mod usage_aggregator;

#[derive(Clone, Debug)]
pub struct StackWithMetadata {
    pub stack: Stack,
    pub revision: u32,
    pub metadata: StackMetadata,
    pub state: StackState, // TODO: don't report out of balance stacks at all?
}

impl StackWithMetadata {
    pub fn id(&self) -> StackID {
        self.metadata.id()
    }
}

#[derive(Clone, Debug)]
pub enum StackState {
    Normal,
    OwnerOutOfBalance,
}

#[derive(Clone, Debug)]
pub enum StackMetadata {
    Solana(SolanaStackMetadata),
}

impl StackMetadata {
    pub fn id(&self) -> StackID {
        match self {
            Self::Solana(solana) => StackID::SolanaPublicKey(solana.account_id.to_bytes()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SolanaStackMetadata {
    pub account_id: Pubkey,
    pub owner: Pubkey,
}
