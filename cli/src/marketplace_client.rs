use std::rc::Rc;

use anchor_client::{
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
        system_program,
        sysvar::{self, rent},
    },
    Program,
};
use anyhow::{bail, Context, Result};
use marketplace::{accounts, instruction, MuState, ProviderRegion};

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

    pub fn create_escrow(&self, user_wallet: Rc<dyn Signer>, provider: Pubkey) -> Result<()> {
        let (mu_state_pda, mu_state) = self.get_mu_state()?;

        let escrow_pda = self.get_escrow_pda(&user_wallet.pubkey(), &provider);

        let accounts = marketplace::accounts::CreateProviderEscrowAccount {
            state: mu_state_pda,
            mint: mu_state.mint,
            escrow_account: escrow_pda,
            provider,
            user: user_wallet.pubkey(),
            system_program: system_program::id(),
            token_program: spl_token::id(),
            rent: rent::id(),
        };

        if !utils::provider_with_keypair_exists(self, &provider)? {
            bail!("There is no provider registered with this keypair");
        }

        if !utils::account_exists(self.program.rpc(), &escrow_pda)? {
            bail!(
                "There is already an escrow_account registered with this user_wallet and provider"
            );
        }

        self.program
            .request()
            .accounts(accounts)
            .args(marketplace::instruction::CreateProviderEscrowAccount {})
            .signer(user_wallet.as_ref())
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                // TODO: what's preflight and what's a preflight commitment?
                skip_preflight: cfg!(debug_assertions),
                ..Default::default()
            })
            .context("Failed to send escrow account creation transaction")?;

        println!("Escrow account created, account key is: {}", escrow_pda);
        println!("Note: to recharge, you can use `mu escrow recharge` or make direct token transfers to this account.");

        Ok(())
    }

    pub fn recharge_escrow(
        &self,
        user_wallet: Rc<dyn Signer>,
        recharge_amount: u64,
        provider: &Pubkey,
        user_token_account: &Pubkey,
    ) -> Result<()> {
        let escrow_pda = self.get_escrow_pda(&user_wallet.pubkey(), provider);

        if !utils::account_exists(self.program.rpc(), &escrow_pda)? {
            bail!("There is no escrow account registered with this user_wallet and provider");
        }

        if !utils::account_exists(self.program.rpc(), user_token_account)? {
            bail!("User token account was not found")
        }

        if utils::get_token_account_balance(self.program.rpc(), user_token_account)?
            < (recharge_amount as f64)
        {
            bail!("User token account has insufficient balance")
        }

        self.program
            .request()
            .instruction(spl_token::instruction::transfer(
                &spl_token::id(),
                user_token_account,
                &escrow_pda,
                &user_wallet.pubkey(),
                &[&user_wallet.pubkey()],
                recharge_amount,
            )?)
            .signer(user_wallet.as_ref())
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                // TODO: what's preflight and what's a preflight commitment?
                skip_preflight: cfg!(debug_assertions),
                ..Default::default()
            })
            .context("Failed to send token transfer transaction")?;

        let account = self.program.rpc().get_token_account_balance(&escrow_pda)?;

        println!(
            "Transfer successful, final escrow balance is: {}",
            account.ui_amount_string
        );

        Ok(())
    }

    pub fn create_region(
        &self,
        accounts: accounts::CreateRegion,
        instruction: instruction::CreateRegion,
        provider_keypair: Rc<dyn Signer>,
    ) -> Result<()> {
        if !utils::provider_with_keypair_exists(self, &provider_keypair.pubkey())? {
            bail!("There is no provider with this key");
        }

        if utils::provider_with_region_exists(
            self,
            &provider_keypair.pubkey(),
            instruction.region_num,
        )? {
            bail!("There is already a region with this provider and region number");
        }

        self.program
            .request()
            .accounts(accounts)
            .args(instruction)
            .signer(provider_keypair.as_ref())
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                // TODO: what's preflight and what's a preflight commitment?
                skip_preflight: cfg!(debug_assertions),
                ..Default::default()
            })
            .context("Failed to send region creation transaction")?;

        Ok(())
    }

    pub fn create_signer(
        &self,
        provider_keypair: Rc<dyn Signer>,
        signer_keypair: &Keypair,
        region_pda: Pubkey,
    ) -> Result<()> {
        let (_, mu_state) = self.get_mu_state()?;

        let (signer_pda, _) = Pubkey::find_program_address(
            &[b"authorized_signer", &region_pda.to_bytes()],
            &self.program.id(),
        );

        let provider_token_account =
            self.get_provider_token_account(provider_keypair.pubkey(), &mu_state);

        let provider_pda = self.get_provider_pda(provider_keypair.pubkey());

        let accounts = marketplace::accounts::CreateAuthorizedUsageSigner {
            provider: provider_pda,
            region: region_pda,
            authorized_signer: signer_pda,
            owner: provider_keypair.pubkey(),
            system_program: system_program::id(),
        };

        if !utils::provider_with_keypair_exists(self, &provider_pda)? {
            bail!("There is no provider with this key");
        }

        if !utils::account_exists(self.program.rpc(), &region_pda)? {
            bail!("There is no region with this region number registered for this provider")
        }

        // TODO: we'd optimally want to let providers have more than one signer
        if utils::signer_for_region_exists(self, &region_pda)? {
            bail!("There is already a signer for this region")
        }

        self.program
            .request()
            .accounts(accounts)
            .args(marketplace::instruction::CreateAuthorizedUsageSigner {
                signer: signer_keypair.pubkey(),
                token_account: provider_token_account,
            })
            .signer(provider_keypair.as_ref())
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                // TODO: what's preflight and what's a preflight commitment?
                skip_preflight: cfg!(debug_assertions),
                ..Default::default()
            })
            .context("Failed to send authorized signer creation transaction")?;

        Ok(())
    }

    pub fn deploy_stack(
        &self,
        accounts: marketplace::accounts::CreateStack,
        instruction: marketplace::instruction::CreateStack,
        user_wallet: Rc<dyn Signer>,
        region: ProviderRegion,
    ) -> Result<()> {
        // TODO: stack update
        let stack_pda = accounts.stack;

        if !utils::provider_with_region_exists(self, &region.provider, region.region_num)? {
            bail!("There is no such region registered with this provider");
        }

        if utils::account_exists(self.program.rpc(), &stack_pda)? {
            bail!("There is already a stack registered with this seed, region and user_wallet")
        }

        self.program
            .request()
            .accounts(accounts)
            .args(instruction)
            .signer(user_wallet.as_ref())
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                // TODO: what's preflight and what's a preflight commitment?
                skip_preflight: cfg!(debug_assertions),
                ..Default::default()
            })
            .context("Failed to send stack creation transaction")?;

        println!("Stack deployed successfully with key: {stack_pda}");

        Ok(())
    }
}

mod utils {
    use anchor_client::{
        solana_client::{
            client_error::ClientErrorKind,
            rpc_client::RpcClient,
            rpc_filter::{Memcmp, RpcFilterType},
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

    pub fn provider_with_region_exists(
        client: &MarketplaceClient,
        provider: &Pubkey,
        region_num: u32,
    ) -> Result<bool> {
        let (pda, _) = Pubkey::find_program_address(
            &[b"region", &provider.to_bytes(), &region_num.to_le_bytes()],
            &client.program.id(),
        );
        account_exists(client.program.rpc(), &pda)
    }

    pub fn signer_for_region_exists(client: &MarketplaceClient, region: &Pubkey) -> Result<bool> {
        let (pda, _) = Pubkey::find_program_address(
            &[b"authorized_signer", &region.to_bytes()],
            &client.program.id(),
        );
        account_exists(client.program.rpc(), &pda)
    }
}
