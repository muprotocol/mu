use super::MarketplaceClient;
use std::rc::Rc;

use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer};
use anyhow::{bail, Context, Result};
use marketplace::{accounts, instruction, ProviderRegion};

pub fn create(
    client: &MarketplaceClient,
    accounts: accounts::CreateRegion,
    instruction: instruction::CreateRegion,
    provider_keypair: Rc<dyn Signer>,
) -> Result<()> {
    if !client.provider_with_keypair_exists(&provider_keypair.pubkey())? {
        bail!("There is no provider with this key");
    }

    if client.provider_with_region_exists(&provider_keypair.pubkey(), instruction.region_num)? {
        bail!("There is already a region with this provider and region number");
    }

    client
        .program
        .request()
        .accounts(accounts)
        .args(instruction)
        .signer(provider_keypair.as_ref())
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send region creation transaction")?;

    Ok(())
}

pub fn get_region(client: &MarketplaceClient, region: Pubkey) -> Result<ProviderRegion> {
    client
        .program
        .account::<marketplace::ProviderRegion>(region)
        .map_err(Into::into)
}

pub fn get_base_url(client: &MarketplaceClient, region: Pubkey) -> Result<String> {
    get_region(client, region).map(|r| r.base_url)
}
