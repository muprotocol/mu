use std::rc::Rc;

    use super::{MarketplaceClient, PROVIDER_INITIALIZATION_FEE};
    use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer, system_program, sysvar};
    use anyhow::{bail, Result};

    pub fn create(
        client: &MarketplaceClient,
        provider_keypair: Rc<dyn Signer>,
        provider_name: String,
    ) -> Result<()> {
        let (state_pda, mu_state) = client.get_mu_state()?;

        let (deposit_pda, _) = Pubkey::find_program_address(&[b"deposit"], &client.program.id());
        let provider_pda = client.get_provider_pda(provider_keypair.pubkey());

        let provider_token_account =
            client.get_provider_token_account(provider_keypair.pubkey(), &mu_state);

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

        if client.provider_with_keypair_exists(&provider_keypair.pubkey())? {
            bail!("There is already a provider registered with this keypair");
        }

        if client.provider_name_exists(&provider_name)? {
            bail!("There is already a provider registered with this name");
        }

        if !client.account_exists(&provider_token_account)? {
            bail!("Token account is not initialized yet.");
        }

        let provider_token_account_balance =
            client.get_token_account_balance(&provider_token_account)?;

        if provider_token_account_balance < PROVIDER_INITIALIZATION_FEE {
            bail!(
                "Token account does not have sufficient balance: needed {}, was {}.",
                PROVIDER_INITIALIZATION_FEE,
                provider_token_account_balance
            );
        }

        client
            .program
            .request()
            .accounts(accounts)
            .args(marketplace::instruction::CreateProvider {
                name: provider_name,
            })
            .signer(provider_keypair.as_ref())
            .send()?;
        Ok(())
    }