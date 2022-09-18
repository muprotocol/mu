//! The functions to perform operations on Solana network

use anchor_client::{solana_sdk::signer::Signer, Cluster};
use anyhow::Result;

/// Solana Client for communicating with Solana network
pub struct SolanaClient {
    anchor_client: anchor_client::Client,
}

impl SolanaClient {
    /// Create new Solana client
    pub fn new(cluster: Cluster, signer: Box<dyn Signer>) -> Result<Self> {
        Ok(Self {
            anchor_client: anchor_client::Client::new(cluster, signer.into()),
        })
    }

    /// Create a new provider
    pub fn create_provider(&self, _name: String) -> Result<()> {
        let _a = &self.anchor_client;
        Ok(())
    }
}
