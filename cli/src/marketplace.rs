//! Communicating with Mu smart contract

use std::rc::Rc;

use anchor_client::{
    solana_sdk::{pubkey::Pubkey, system_program, sysvar},
    Program,
};
use anyhow::{Context, Result};
use marketplace::MuState;
use spl_associated_token_account::get_associated_token_address;

use crate::config::Config;

/// Marketplace Client for communicating with Mu smart contracts
pub struct MarketplaceClient {
    program: Program,
}

impl MarketplaceClient {
    /// Create new Solana client with provided config
    pub fn new(config: Config) -> Result<Self> {
        let wallet = config.wallet_kp()?;
        Ok(Self {
            program: anchor_client::Client::new(config.cluster, Rc::new(wallet))
                .program(config.program_id),
        })
    }

    /// Create a new provider
    pub fn create_provider(&self, name: String) -> Result<()> {
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &self.program.id());
        let (deposit_pda, _) = Pubkey::find_program_address(&[b"deposit"], &self.program.id());
        let (provider_pda, _) = Pubkey::find_program_address(
            &[b"deposit", &self.program.payer().to_bytes()],
            &self.program.id(),
        );

        let mu_state: MuState = self.program.account(state_pda)?;

        let provider_token_account =
            get_associated_token_address(&self.program.payer(), &mu_state.mint);

        let accounts = marketplace::accounts::CreateProvider {
            state: state_pda,
            provider: provider_pda,
            deposit_token: deposit_pda,
            owner: self.program.payer(),
            owner_token: provider_token_account,
            system_program: system_program::id(),
            token_program: spl_token::id(),
            rent: sysvar::rent::id(),
        };

        let a = self
            .program
            .request()
            .args(marketplace::instruction::CreateProvider { name })
            .accounts(accounts)
            .send()
            .context("error in creating provider")?;

        println!("Sig: {}", a);
        Ok(())
    }
}
