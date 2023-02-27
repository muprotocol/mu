use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer, system_program};
use anyhow::{bail, Context, Result};

use super::MarketplaceClient;

pub fn initialize(
    client: &MarketplaceClient,
    signer: &dyn Signer,
    token_mint: Pubkey,
    commission_rate_micros: u32,
    provider_deposit: u64,
) -> Result<()> {
    let state_pda = client.get_mu_state_pda();
    if client.account_exists(&state_pda)? {
        bail!("Already initialized");
    }

    if !client.account_exists(&token_mint)? {
        bail!("Mint is not initialized");
    }

    let (deposit_token, _) = Pubkey::find_program_address(&[b"deposit"], &marketplace::id());
    let (commission_token, _) = Pubkey::find_program_address(&[b"commission"], &marketplace::id());

    client
        .program
        .request()
        .args(marketplace::instruction::Initialize {
            commission_rate_micros,
            provider_deposit,
        })
        .accounts(marketplace::accounts::Initialize {
            authority: signer.pubkey(),
            commission_token,
            deposit_token,
            mint: token_mint,
            state: client.get_mu_state_pda(),
            system_program: system_program::id(),
            token_program: spl_token::id(),
        })
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send initialization transaction")?;

    Ok(())
}

pub fn create_provider_authorizer(
    client: &MarketplaceClient,
    authority: &dyn Signer,
    authorizer: &dyn Signer,
) -> Result<()> {
    let state_pda = client.get_mu_state_pda();
    let (authorizer_pda, _) = Pubkey::find_program_address(
        &[b"authorizer", &authorizer.pubkey().to_bytes()[..]],
        &marketplace::id(),
    );
    if client.account_exists(&authorizer_pda)? {
        bail!("Provider authorizer already exists");
    }

    client
        .program
        .request()
        .args(marketplace::instruction::CreateProviderAuthorizer {})
        .accounts(marketplace::accounts::CreateProviderAuthorizer {
            state: state_pda,
            provider_authorizer: authorizer_pda,
            authority: authority.pubkey(),
            authorizer: authorizer.pubkey(),
            system_program: system_program::id(),
        })
        .signer(authorizer)
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send authorizer creation transaction")?;

    Ok(())
}

pub fn authorize_provider(
    client: &MarketplaceClient,
    authorizer: &dyn Signer,
    provider_owner: Pubkey,
) -> Result<()> {
    let provider_pda = client.get_provider_pda(provider_owner);
    let (authorizer_pda, _) = Pubkey::find_program_address(
        &[b"authorizer", &authorizer.pubkey().to_bytes()[..]],
        &marketplace::id(),
    );
    if !client.account_exists(&authorizer_pda)? {
        bail!("Provider authorizer doesn't exist");
    }

    client
        .program
        .request()
        .args(marketplace::instruction::AuthorizeProvider {})
        .accounts(marketplace::accounts::AuthorizeProvider {
            provider_authorizer: authorizer_pda,
            authorizer: authorizer.pubkey(),
            provider: provider_pda,
            owner: provider_owner,
        })
        .signer(authorizer)
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send provider authorization transaction")?;

    Ok(())
}
