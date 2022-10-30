use std::rc::Rc;

use anchor_client::{solana_sdk::pubkey::Pubkey, Program};
use anyhow::Result;
use marketplace::MuState;

use crate::config::Config;

/// Marketplace Client for communicating with Mu smart contracts
pub struct MarketplaceClient {
    pub program: Program,
}

impl MarketplaceClient {
    /// Create new Solana client with provided config
    pub fn new(config: &Config) -> Result<Self> {
        let payer = config.payer_kp()?;
        Ok(Self {
            program: anchor_client::Client::new(config.cluster.clone(), Rc::new(payer))
                .program(config.program_id), // TODO: use program ID from marketplace package, handle dev v.s. prod there
        })
    }

    pub fn get_mu_state_pda(&self) -> Pubkey {
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &self.program.id());
        state_pda
    }

    pub fn get_mu_state(&self) -> Result<(Pubkey, MuState)> {
        let state_pda = self.get_mu_state_pda();
        let mu_state: MuState = self.program.account(state_pda)?;
        Ok((state_pda, mu_state))
    }

    pub fn get_provider_pda(&self, provider_wallet: Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[b"provider", &provider_wallet.to_bytes()],
            &self.program.id(),
        )
        .0
    }

    pub fn get_provider_token_account(
        &self,
        provider_wallet: Pubkey,
        mu_state: &MuState,
    ) -> Pubkey {
        spl_associated_token_account::get_associated_token_address(&provider_wallet, &mu_state.mint)
    }

    pub fn get_region_pda(&self, provider_wallet: &Pubkey, region_num: u32) -> Pubkey {
        let (region_pda, _) = Pubkey::find_program_address(
            &[
                b"region",
                &provider_wallet.to_bytes(),
                &region_num.to_le_bytes(),
            ],
            &self.program.id(),
        );
        region_pda
    }

    pub fn get_escrow_pda(&self, user_wallet: &Pubkey, provider_pda: &Pubkey) -> Pubkey {
        let (escrow_pda, _) = Pubkey::find_program_address(
            &[b"escrow", &user_wallet.to_bytes(), &provider_pda.to_bytes()],
            &self.program.id(),
        );
        escrow_pda
    }
}
