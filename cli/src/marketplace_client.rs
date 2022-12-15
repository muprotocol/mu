use std::rc::Rc;

use anchor_client::{
    solana_sdk::{pubkey::Pubkey, signer::Signer, system_program, sysvar},
    Program,
};
use anyhow::{bail, Result};
use marketplace::MuState;

use crate::config::Config;

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

        if utils::provider_with_keypair_exists(self, &provider_keypair.pubkey())? {
            bail!("There is already a provider registered with this keypair");
        }

        if utils::provider_name_exists(self, &provider_name)? {
            bail!("There is already a provider registered with this name");
        }

        if !utils::account_exists(self.program.rpc(), &provider_token_account)? {
            bail!("Token account is not initialized yet.");
        }

        let provider_token_account_balance =
            utils::get_token_account_balance(self.program.rpc(), &provider_token_account)?;

        if provider_token_account_balance < PROVIDER_INITIALIZATION_FEE {
            bail!(
                "Token account does not have sufficient balance: needed {}, was {}.",
                PROVIDER_INITIALIZATION_FEE,
                provider_token_account_balance
            );
        }

        self.program
            .request()
            .accounts(accounts)
            .args(marketplace::instruction::CreateProvider {
                name: provider_name,
            })
            .signer(provider_keypair.as_ref())
            .send()?;
        Ok(())
    }
}

mod utils {
    use anchor_client::{
        solana_client::{
            client_error::ClientErrorKind,
            rpc_client::RpcClient,
            rpc_filter::{Memcmp, MemcmpEncodedBytes, MemcmpEncoding, RpcFilterType},
            rpc_request::RpcError,
        },
        solana_sdk::pubkey::Pubkey,
    };
    use anyhow::{anyhow, Result};

    use super::MarketplaceClient;

    pub fn account_exists(rpc: RpcClient, pubkey: &Pubkey) -> Result<bool> {
        match rpc.get_account(pubkey) {
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

    pub fn get_token_account_balance(rpc: RpcClient, pubkey: &Pubkey) -> Result<f64> {
        let info = rpc.get_token_account_balance(pubkey)?;
        let amount: f64 = info.amount.parse()?;

        Ok(amount / 10u32.pow(info.decimals.into()) as f64)
    }

    pub fn provider_name_exists(client: &MarketplaceClient, name: &str) -> Result<bool> {
        let name_len: u64 = name
            .len()
            .try_into()
            .map_err(|e| anyhow!("provider name too long: {e}"))?;

        let filters = vec![
            RpcFilterType::Memcmp(Memcmp {
                offset: 8,
                bytes: MemcmpEncodedBytes::Bytes(vec![marketplace::MuAccountType::Provider as u8]),
                encoding: Some(MemcmpEncoding::Binary),
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 8 + 1 + 32 + 4, // 4 more bytes for the prefix length
                bytes: MemcmpEncodedBytes::Bytes(name.as_bytes().to_vec()),
                encoding: Some(MemcmpEncoding::Binary),
            }),
            RpcFilterType::DataSize(
                // Account type and etc
                8 + 1 + 32
                // name: String Size + String length
                + 4 + name_len
                // End of account data
                + 1,
            ),
        ];

        let accounts = client.program.accounts::<marketplace::Provider>(filters)?;

        Ok(!accounts.is_empty())
    }

    pub fn provider_with_keypair_exists(
        client: &MarketplaceClient,
        pubkey: &Pubkey,
    ) -> Result<bool> {
        let (pda, _) =
            Pubkey::find_program_address(&[b"provider", &pubkey.to_bytes()], &client.program.id());
        account_exists(client.program.rpc(), &pda)
    }
}
