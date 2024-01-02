use mu_stack::{StackID, StackOwner, ValidatedStack};
use pwr_rs::wallet::PublicKey;

pub mod blockchain_monitor;
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
    PWR(PWRStackMetadata),
}

impl StackMetadata {
    pub fn id(&self) -> StackID {
        match self {
            Self::PWR(p) => StackID::PWRStackID(p.stack_id),
        }
    }

    pub fn owner(&self) -> StackOwner {
        match self {
            Self::PWR(p) => StackOwner::PWR(p.owner),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PWRStackMetadata {
    pub stack_id: uuid::Uuid,
    pub owner: PublicKey,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum ApiRequestSigner {
    PWR(PublicKey),
}
