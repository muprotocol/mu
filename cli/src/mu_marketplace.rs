//! Communicating with Mu smart contracts

use anchor_client::{
    solana_sdk::{signer::Signer, system_program},
    Cluster, Program,
};
use anyhow::Result;

/// Marketplace Client for communicating with Mu smart contracts
pub struct MarketplaceClient {
    program: Program,
}

impl MarketplaceClient {
    /// Create new Solana client
    pub fn new(cluster: Cluster, signer: Box<dyn Signer>) -> Result<Self> {
        Ok(Self {
            program: anchor_client::Client::new(cluster, signer.into()).program(marketplace::id()),
        })
    }

    /// Create a new provider
    pub fn create_provider(&self, name: String) -> Result<()> {
        let accounts = marketplace::accounts::CreateProvider {
            state: todo!(),
            provider: todo!(),
            deposit_token: todo!(),
            owner: todo!(),
            owner_token: todo!(),
            system_program: system_program::ID,
            token_program: rent: todo!(),
        };

        self.program
            .request()
            .args(marketplace::instruction::CreateProvider { name })
            .accounts(accounts)
            .send()?;
        Ok(())
    }
}
