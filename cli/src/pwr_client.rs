use anyhow::{anyhow, Result};
use pwr_rs::{rpc::RPC, PublicKey};

use crate::config::Config;

#[cfg(feature = "admin")]
pub mod admin;

//pub mod escrow;
pub mod provider;
//pub mod region;
pub mod request_signer;
//pub mod signer;
pub mod stack;

/// PWR Client for communicating with Mu executor
pub struct PWRClient {
    rpc: RPC,
}

impl PWRClient {
    /// Create new PWR client with provided config
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            // TODO: read from config
            rpc: pwr_rs::rpc::RPC::new("https://pwrrpc.pwrlabs.io")
                .map_err(|e| anyhow!("Failed to create RPC: {e}"))?,
        })
    }

    pub fn user_stacks(&self, user_wallet: &PublicKey) -> Reuslt {
        let (pda, _) = Pubkey::find_program_address(
            &[
                b"request_signer",
                &user_wallet.to_bytes(),
                &signer.to_bytes(),
                &region_pda.to_bytes(),
            ],
            &self.program.id(),
        );
        pda
    }
}
