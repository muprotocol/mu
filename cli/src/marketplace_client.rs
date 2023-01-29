use anchor_client::{
    solana_client::{
        client_error::ClientErrorKind,
        rpc_filter::{Memcmp, RpcFilterType},
        rpc_request::RpcError,
    },
    solana_sdk::{program_pack::Pack, pubkey::Pubkey},
    Program,
};
use anyhow::{anyhow, Context, Result};
use marketplace::MuState;
use spl_token::state::Mint;

use crate::config::Config;

pub mod escrow;
pub mod provider;
pub mod region;
pub mod signer;
pub mod stack;

const PROVIDER_INITIALIZATION_FEE: f64 = 100.0; //TODO: This needs to be read from
                                                //blockchain

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

    pub fn get_mint(&self, mu_state: &MuState) -> Result<Mint> {
        let mint_account = self.program.rpc().get_account(&mu_state.mint)?;
        <Mint as Pack>::unpack(&mint_account.data).context("Failed to parse mint account data")
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

    pub fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        match self.program.rpc().get_account(pubkey) {
            Ok(_) => Ok(true),
            Err(client_error) => match client_error.kind {
                ClientErrorKind::RpcError(RpcError::ForUser(s))
                    if s.contains("AccountNotFound") =>
                {
                    Ok(false)
                }
                _ => Err(client_error.into()),
            },
        }
    }

    pub fn get_token_account_balance(&self, pubkey: &Pubkey) -> Result<f64> {
        let info = self.program.rpc().get_token_account_balance(pubkey)?;
        let amount: f64 = info.amount.parse()?;

        Ok(amount / 10u32.pow(info.decimals.into()) as f64)
    }

    pub fn provider_name_exists(&self, name: &str) -> Result<bool> {
        let name_len: u64 = name
            .len()
            .try_into()
            .map_err(|e| anyhow!("provider name too long: {e}"))?;

        let filters = vec![
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                8,
                vec![marketplace::MuAccountType::Provider as u8],
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                8 + 1 + 32 + 1 + 4, // 4 more bytes for the prefix length
                name.as_bytes().to_vec(),
            )),
            RpcFilterType::DataSize(
                // Account type and etc
                8 + 1 + 32 + 1
                // name: String Size + String length
                + 4 + name_len
                // End of account data
                + 1,
            ),
        ];

        let accounts = self.program.accounts::<marketplace::Provider>(filters)?;

        Ok(!accounts.is_empty())
    }

    pub fn provider_with_keypair_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        let (pda, _) =
            Pubkey::find_program_address(&[b"provider", &pubkey.to_bytes()], &self.program.id());
        self.account_exists(&pda)
    }

    pub fn provider_with_region_exists(&self, provider: &Pubkey, region_num: u32) -> Result<bool> {
        let (pda, _) = Pubkey::find_program_address(
            &[b"region", &provider.to_bytes(), &region_num.to_le_bytes()],
            &self.program.id(),
        );
        self.account_exists(&pda)
    }

    pub fn signer_for_region_exists(&self, region: &Pubkey) -> Result<bool> {
        let (pda, _) = Pubkey::find_program_address(
            &[b"authorized_signer", &region.to_bytes()],
            &self.program.id(),
        );
        self.account_exists(&pda)
    }
}
