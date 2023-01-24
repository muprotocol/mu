use std::rc::Rc;

use anchor_client::{
    solana_client::rpc_config::RpcSendTransactionConfig, solana_sdk::signer::Signer,
};
use anyhow::{bail, Context, Result};
use marketplace::ProviderRegion;

use super::MarketplaceClient;

pub fn deploy_stack(
    client: &MarketplaceClient,
    accounts: marketplace::accounts::CreateStack,
    instruction: marketplace::instruction::CreateStack,
    user_wallet: Rc<dyn Signer>,
    region: ProviderRegion,
) -> Result<()> {
    // TODO: stack update
    let stack_pda = accounts.stack;

    if !client.provider_with_region_exists(&region.provider, region.region_num)? {
        bail!("There is no such region registered with this provider");
    }

    if client.account_exists(&stack_pda)? {
        bail!("There is already a stack registered with this seed, region and user_wallet")
    }

    client
        .program
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
