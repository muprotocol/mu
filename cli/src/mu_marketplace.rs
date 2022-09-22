//! Communicating with Mu smart contracts

use anchor_client::{
    solana_sdk::{pubkey::Pubkey, signer::Signer, system_program, sysvar},
    Cluster, Program,
};
use anchor_spl::associated_token::get_associated_token_address;
use anyhow::Result;
use marketplace::MuState;

/// Marketplace Client for communicating with Mu smart contracts
pub struct MarketplaceClient {
    program: Program,
}

impl MarketplaceClient {
    /// Create new Solana client
    pub fn new(cluster: Cluster, owner: Box<dyn Signer>) -> Result<Self> {
        Ok(Self {
            program: anchor_client::Client::new(cluster, owner.into()).program(marketplace::id()),
        })
    }

    fn fetch_program_state(&self) -> Result<MuState> {
        self.program.state().map_err(Into::into)
    }

    fn program_mint_address(&self) -> Result<Pubkey> {
        self.fetch_program_state().map(|s| s.mint)
    }

    /// Create a new provider
    pub fn create_provider(&self, name: String) -> Result<()> {
        let provider_token_account =
            get_associated_token_address(&self.program.payer(), &self.program_mint_address()?);

        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &self.program.id());
        let (deposit_pda, _) = Pubkey::find_program_address(&[b"deposit"], &self.program.id());
        let (provider_pda, _) = Pubkey::find_program_address(
            &[b"deposit", &self.program.payer().to_bytes()],
            &self.program.id(),
        );

        let accounts = marketplace::accounts::CreateProvider {
            state: state_pda,
            provider: provider_pda,
            deposit_token: deposit_pda,
            owner: self.program.payer(),
            owner_token: provider_token_account,
            system_program: system_program::ID,
            token_program: anchor_spl::token::ID,
            rent: sysvar::rent::ID,
        };

        let a = self
            .program
            .request()
            .args(marketplace::instruction::CreateProvider { name })
            .accounts(accounts)
            .send()?;

        println!("Sig: {}", a);
        Ok(())
    }
}
