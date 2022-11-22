use std::rc::Rc;

use anchor_client::{
    solana_sdk::{
        instruction::InstructionError, pubkey::Pubkey, signer::Signer, system_program, sysvar,
    },
    Program,
};
use anyhow::{anyhow, Result};
use marketplace::MuState;

use crate::{
    config::Config,
    error::{CliError, MarketplaceResultExt},
};

/// Marketplace Client for communicating with Mu smart contracts
pub struct MarketplaceClient {
    pub program: Program,
}

impl MarketplaceClient {
    /// Create new Solana client with provided config
    pub fn new(config: &Config) -> Result<Self> {
        let payer = config.get_signer()?;
        Ok(Self {
            program: anchor_client::Client::new(config.cluster.clone(), payer)
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

    pub fn get_stack_pda(&self, user_wallet: Pubkey, region_pda: Pubkey, seed: u64) -> Pubkey {
        let (stack_pda, _) = Pubkey::find_program_address(
            &[
                b"stack",
                &user_wallet.to_bytes(),
                &region_pda.to_bytes(),
                &seed.to_le_bytes(),
            ],
            &self.program.id(),
        );
        stack_pda
    }

    pub fn create_provider(
        &self,
        provider_keypair: Rc<dyn Signer>,
        provider_name: String,
    ) -> Result<()> {
        let (state_pda, mu_state) = self.get_mu_state()?;

        let (deposit_pda, _) = Pubkey::find_program_address(&[b"deposit"], &self.program.id());
        let provider_pda = self.get_provider_pda(provider_keypair.pubkey());

        // TODO: we need to double-check all error conditions and generate user-readable error messages.
        // there is no backend server to return cute messages, only the deep, dark bowels of the blockchain.
        let provider_token_account =
            self.get_provider_token_account(provider_keypair.pubkey(), &mu_state);

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

        if utils::token_account_is_initialized(self.program.rpc(), &provider_token_account)? {
            self.program
                .request()
                .accounts(accounts)
                .args(marketplace::instruction::CreateProvider {
                    name: provider_name,
                })
                .signer(provider_keypair.as_ref())
                .send()
                .parse_error(|e| match e {
                    CliError::InstructionError(i, e) => match (i, e) {
                        (0, InstructionError::Custom(0xbc4)) => {
                            anyhow!("Provider token account not initialized")
                        }
                        (0, InstructionError::Custom(0x1)) => {
                            anyhow!("Provider token account does not have sufficient balance")
                        }
                        (_, e) => e.into(),
                    },
                    CliError::UnexpectedError(e) => e,
                    CliError::UnhandledError(e) => e.into(),
                })?;
        } else {
            println!("Token account is not initialized yet.");
        }

        Ok(())
    }
}

mod utils {
    use anchor_client::{solana_client::rpc_client::RpcClient, solana_sdk::pubkey::Pubkey};
    use anyhow::Result;

    pub fn token_account_is_initialized(rpc: RpcClient, pubkey: &Pubkey) -> Result<bool> {
        rpc.get_token_account(pubkey)
            .map_err(Into::into)
            .map(|i| i.is_some())
    }
}
