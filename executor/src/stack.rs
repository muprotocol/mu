use mu_stack::{StackID, ValidatedStack};
use solana_sdk::pubkey::Pubkey;

pub mod blockchain_monitor;
mod config_types;
pub mod deploy;
pub mod request_signer_cache;
pub mod scheduler;
pub mod usage_aggregator;

#[derive(Clone, Debug)]
pub struct StackWithMetadata {
    pub stack: ValidatedStack,
    pub name: String,
    pub revision: u32,
    pub metadata: StackMetadata,
}

impl StackWithMetadata {
    pub fn id(&self) -> StackID {
        self.metadata.id()
    }

    pub fn owner(&self) -> StackOwner {
        self.metadata.owner()
    }
}

#[derive(Clone, Debug)]
pub enum StackMetadata {
    Solana(SolanaStackMetadata),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StackOwner {
    Solana(Pubkey),
}

impl StackOwner {
    // TODO: violates multi-chain
    pub fn to_inner(&self) -> Pubkey {
        let StackOwner::Solana(pk) = self;
        *pk
    }
}

impl StackMetadata {
    pub fn id(&self) -> StackID {
        match self {
            Self::Solana(solana) => StackID::SolanaPublicKey(solana.account_id.to_bytes()),
        }
    }

    pub fn owner(&self) -> StackOwner {
        match self {
            Self::Solana(solana) => StackOwner::Solana(solana.owner),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SolanaStackMetadata {
    pub account_id: Pubkey,
    pub owner: Pubkey,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum ApiRequestSigner {
    Solana(Pubkey),
}
