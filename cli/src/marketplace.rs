//! Communicating with Mu smart contract

use std::rc::Rc;

use anchor_client::{
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{pubkey::Pubkey, signer::Signer, system_program, sysvar},
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
    // TODO: why consume the config?
    pub fn new(config: Config) -> Result<Self> {
        let payer = config.payer_kp()?;
        Ok(Self {
            program: anchor_client::Client::new(config.cluster, Rc::new(payer))
                .program(config.program_id), // TODO: use program ID from marketplace package, handle dev v.s. prod there
        })
    }

    /// Create a new provider
    // TODO: should this code be moved to the command itself to keep this module short?
    // since most commands deal with the smart contracts in some way, this module is
    // likely to contain most code in the program.
    pub fn create_provider(&self, name: String, provider_keypair: impl Signer) -> Result<()> {
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &self.program.id()); // TODO: repeated &self.program.id()
        let (deposit_pda, _) = Pubkey::find_program_address(&[b"deposit"], &self.program.id());
        let (provider_pda, _) = Pubkey::find_program_address(
            &[b"provider", &provider_keypair.pubkey().to_bytes()],
            &self.program.id(),
        );

        let mu_state: MuState = self.program.account(state_pda)?;

        // TODO: we need to double-check all error conditions and generate user-readable error messages.
        // there is no backend server to return cute messages, only the deep, dark bowels of the blockchain.
        let provider_token_account =
            get_associated_token_address(&provider_keypair.pubkey(), &mu_state.mint);

        let accounts = marketplace::accounts::CreateProvider {
            state: state_pda,
            provider: provider_pda,
            deposit_token: deposit_pda,
            owner: provider_keypair.pubkey(),
            owner_token: provider_token_account,
            system_program: system_program::id(),
            token_program: spl_token::id(),
            rent: sysvar::rent::id(),
        };

        self.program
            .request()
            .accounts(accounts)
            .args(marketplace::instruction::CreateProvider { name })
            .signer(&provider_keypair)
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                // TODO: what's preflight and what's a preflight commitment?
                skip_preflight: cfg!(debug_assertions),
                preflight_commitment: None,
                encoding: None,
                max_retries: None,
                min_context_slot: None,
            })
            .context("error in creating provider")?;
        Ok(())
    }
}
