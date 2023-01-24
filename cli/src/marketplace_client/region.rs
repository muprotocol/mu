use super::MarketplaceClient;
use std::rc::Rc;

use anchor_client::{
    solana_client::rpc_config::RpcSendTransactionConfig, solana_sdk::signer::Signer,
};
use anyhow::{bail, Context, Result};
use marketplace::{accounts, instruction};

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
        .send_with_spinner_and_config(RpcSendTransactionConfig {
            // TODO: what's preflight and what's a preflight commitment?
            skip_preflight: cfg!(debug_assertions),
            ..Default::default()
        })
        .context("Failed to send region creation transaction")?;

    Ok(())
}
