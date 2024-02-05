use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer, system_program};
use anyhow::{bail, Context, Result};

use super::MarketplaceClient;

pub fn create(
    client: &MarketplaceClient,
    user_wallet: &dyn Signer,
    signer: &dyn Signer,
    region_pda: &Pubkey,
) -> Result<Pubkey> {
    let pda = client.get_request_signer_pda(&user_wallet.pubkey(), &signer.pubkey(), region_pda);

    if !client.account_exists(region_pda)? {
        bail!("Region does not exist");
    }

    if client.account_exists(&pda)? {
        bail!("API request signer already exists, use the activate and deactivate subcommands to manage its state");
    }

    let accounts = marketplace::accounts::CreateApiRequestSigner {
        request_signer: pda,
        user: user_wallet.pubkey(),
        signer: signer.pubkey(),
        region: *region_pda,
        system_program: system_program::id(),
    };
    let instruction = marketplace::instruction::CreateApiRequestSigner {};
    client
        .program
        .request()
        .accounts(accounts)
        .args(instruction)
        .signer(user_wallet)
        .signer(signer)
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send API request signer creation transaction")?;

    Ok(pda)
}

pub fn activate(
    client: &MarketplaceClient,
    user_wallet: &dyn Signer,
    signer: &dyn Signer,
    region_pda: &Pubkey,
) -> Result<Pubkey> {
    let pda = client.get_request_signer_pda(&user_wallet.pubkey(), &signer.pubkey(), region_pda);

    if !client.account_exists(&pda)? {
        bail!("API request signer doesn't exist");
    }

    let accounts = marketplace::accounts::ActivateApiRequestSigner {
        request_signer: pda,
        user: user_wallet.pubkey(),
        signer: signer.pubkey(),
        region: *region_pda,
    };
    let instruction = marketplace::instruction::ActivateApiRequestSigner {};
    client
        .program
        .request()
        .accounts(accounts)
        .args(instruction)
        .signer(user_wallet)
        .signer(signer)
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send API request signer activation transaction")?;

    Ok(pda)
}

pub fn deactivate(
    client: &MarketplaceClient,
    user_wallet: &dyn Signer,
    signer: &Pubkey,
    region_pda: &Pubkey,
) -> Result<Pubkey> {
    let pda = client.get_request_signer_pda(&user_wallet.pubkey(), signer, region_pda);

    if !client.account_exists(&pda)? {
        bail!("API request signer doesn't exist");
    }

    let accounts = marketplace::accounts::DeactivateApiRequestSigner {
        request_signer: pda,
        user: user_wallet.pubkey(),
        signer: *signer,
        region: *region_pda,
    };
    let instruction = marketplace::instruction::DeactivateApiRequestSigner {};
    client
        .program
        .request()
        .accounts(accounts)
        .args(instruction)
        .signer(user_wallet)
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send API request signer deactivation transaction")?;

    Ok(pda)
}
