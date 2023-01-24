use std::rc::Rc;

use anchor_client::{
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{pubkey::Pubkey, signer::Signer, system_program, sysvar::rent},
};
use anyhow::{bail, Context, Result};

use super::MarketplaceClient;

pub fn create(
    client: &MarketplaceClient,
    user_wallet: Rc<dyn Signer>,
    provider: Pubkey,
) -> Result<()> {
    let (mu_state_pda, mu_state) = client.get_mu_state()?;

    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), &provider);

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

    if !client.provider_with_keypair_exists(&provider)? {
        bail!("There is no provider registered with this keypair");
    }

    if client.account_exists(&escrow_pda)? {
        bail!("There is already an escrow_account registered with this user_wallet and provider");
    }

    client
        .program
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

pub fn recharge(
    client: &MarketplaceClient,
    user_wallet: Rc<dyn Signer>,
    recharge_amount: u64,
    provider: &Pubkey,
    user_token_account: &Pubkey,
) -> Result<()> {
    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), provider);

    if !client.account_exists(&escrow_pda)? {
        bail!("There is no escrow account registered with this user_wallet and provider");
    }

    if !client.account_exists(user_token_account)? {
        bail!("User token account was not found")
    }

    if client.get_token_account_balance(user_token_account)? < (recharge_amount as f64) {
        bail!("User token account has insufficient balance")
    }

    client
        .program
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

    let account = client
        .program
        .rpc()
        .get_token_account_balance(&escrow_pda)?;

    println!(
        "Transfer successful, final escrow balance is: {}",
        account.ui_amount_string
    );

    Ok(())
}
