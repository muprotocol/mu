use std::rc::Rc;

use super::MarketplaceClient;
use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer, system_program};
use anyhow::{bail, Context, Result};

pub fn create(
    client: &MarketplaceClient,
    provider_keypair: Rc<dyn Signer>,
    signer_keypair: Rc<dyn Signer>,
    region_num: u32,
) -> Result<()> {
    let (_, mu_state) = client.get_mu_state()?;

    let provider_pda = client.get_provider_pda(provider_keypair.pubkey());
    let region_pda = client.get_region_pda(&provider_keypair.pubkey(), region_num);

    let (signer_pda, _) = Pubkey::find_program_address(
        &[b"authorized_signer", &region_pda.to_bytes()],
        &client.program.id(),
    );

    let provider_token_account =
        client.get_provider_token_account(provider_keypair.pubkey(), &mu_state);

    let accounts = marketplace::accounts::CreateAuthorizedUsageSigner {
        provider: provider_pda,
        region: region_pda,
        authorized_signer: signer_pda,
        owner: provider_keypair.pubkey(),
        system_program: system_program::id(),
    };

    if !client.provider_with_keypair_exists(&provider_keypair.pubkey())? {
        bail!(
            "There is no provider with this key (wallet: {}, PDA: {})",
            provider_keypair.pubkey(),
            provider_pda
        );
    }

    if !client.account_exists(&region_pda)? {
        bail!("There is no region with this region number registered for this provider")
    }

    // TODO: we'd optimally want to let providers have more than one signer
    if client.signer_for_region_exists(&region_pda)? {
        bail!("There is already a signer for this region")
    }

    client
        .program
        .request()
        .accounts(accounts)
        .args(marketplace::instruction::CreateAuthorizedUsageSigner {
            signer: signer_keypair.pubkey(),
            token_account: provider_token_account,
        })
        .signer(provider_keypair.as_ref())
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send authorized signer creation transaction")?;

    Ok(())
}
