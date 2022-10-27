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

    pub fn get_mu_state(&self) -> Result<(Pubkey, MuState)> {
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &self.program.id());
        let mu_state: MuState = self.program.account(state_pda)?;
        Ok((state_pda, mu_state))
    }

    pub fn get_provider_token_account(&self, provider: Pubkey, mu_state: &MuState) -> Pubkey {
        spl_associated_token_account::get_associated_token_address(&provider, &mu_state.mint)
    }

    pub fn get_region_pda(&self, provider: Pubkey, region_num: u32) -> Pubkey {
        let (region_pda, _) = Pubkey::find_program_address(
            &[b"region", &provider.to_bytes(), &region_num.to_le_bytes()],
            &self.program.id(),
        );
        region_pda
    }
}
