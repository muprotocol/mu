use std::{collections::HashSet, process::exit, rc::Rc};

use anchor_client::{
    solana_client::{
        client_error::{ClientError, ClientErrorKind},
        rpc_filter::{Memcmp, RpcFilterType},
        rpc_request::RpcError,
    },
    solana_sdk::{pubkey::Pubkey, signer::Signer, system_program, sysvar::rent},
};
use anyhow::{bail, Context, Result};
use marketplace::{Provider, ProviderRegion};
use solana_account_decoder::parse_token::UiTokenAmount;
use spl_associated_token_account::get_associated_token_address;

use crate::token_utils::{token_amount_to_ui_amount, ui_amount_to_token_amount};

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
        .send_with_spinner_and_config(Default::default())
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
        .send_with_spinner_and_config(Default::default())
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

pub fn withdraw(
    client: &MarketplaceClient,
    user_wallet: Rc<dyn Signer>,
    provider_pda: &Pubkey,
    withdraw_amount: f64,
    withdraw_to: Option<Pubkey>,
    force: bool,
) -> Result<()> {
    let (state_pda, mu_state) = client.get_mu_state()?;
    let mint = client.get_mint(&mu_state)?;

    let withdraw_amount = ui_amount_to_token_amount(&mint, withdraw_amount);

    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), provider_pda);
    let token_account = get_escrow_balance(client, &escrow_pda)?;
    let balance: u64 = token_account.amount.parse().unwrap();

    if balance < withdraw_amount {
        println!(
            "Escrow account balance is less than specified amount. Balance is: {}",
            token_amount_to_ui_amount(&mint, balance)
        );
        exit(-1);
    }

    let verify_result = get_regions_where_balance_is_below_minimum(
        client,
        provider_pda,
        &user_wallet.pubkey(),
        balance - withdraw_amount,
    )?;

    let regions_below_minimum_with_stacks = verify_result
        .regions_below_minimum
        .iter()
        .filter(|x| x.user_has_stacks)
        .collect::<Vec<_>>();

    if !regions_below_minimum_with_stacks.is_empty() && !force {
        println!("Withdrawing this amount will result in escrow balance going below minimum for regions with active stack deployments.");
        println!(
            "Escrow balance: {}",
            token_amount_to_ui_amount(&mint, balance)
        );
        println!(
            "Remaining after withdraw: {}",
            token_amount_to_ui_amount(&mint, balance - withdraw_amount)
        );
        println!("Regions:");
        for region in regions_below_minimum_with_stacks {
            println!(
                "\t{}: Minimum escrow balance is {}",
                region.region.name,
                token_amount_to_ui_amount(&mint, region.region.min_escrow_balance)
            )
        }
        println!("Specify --force if you want to withdraw anyway.");
        exit(-1);
    }

    let withdraw_to = withdraw_to
        .unwrap_or_else(|| get_associated_token_address(&user_wallet.pubkey(), &mu_state.mint));

    let accounts = marketplace::accounts::WithdrawEscrow {
        state: state_pda,
        escrow_account: escrow_pda,
        user: user_wallet.pubkey(),
        provider: *provider_pda,
        withdraw_to,
        token_program: spl_token::id(),
    };

    client
        .program
        .request()
        .accounts(accounts)
        .args(marketplace::instruction::WithdrawEscrowBalance {
            amount: withdraw_amount,
        })
        .signer(user_wallet.as_ref())
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send escrow withdraw transaction")?;

    Ok(())
}

pub fn get_escrow_balance(
    client: &MarketplaceClient,
    escrow_pda: &Pubkey,
) -> Result<UiTokenAmount> {
    match client.program.rpc().get_token_account_balance(escrow_pda) {
        Ok(token_account) => Ok(token_account),
        Err(ClientError {
            kind: ClientErrorKind::RpcError(RpcError::RpcResponseError { code: -32602, .. }),
            ..
        }) => bail!("Escrow account does not exist"),
        Err(f) => Err(f).context("Failed to fetch escrow account balance"),
    }
}

pub struct RegionBalanceInfo {
    pub region: ProviderRegion,
    pub user_has_stacks: bool,
}

pub struct VerifyBalanceResult {
    pub provider: Provider,
    pub regions_below_minimum: Vec<RegionBalanceInfo>,
}

pub fn get_regions_where_balance_is_below_minimum(
    client: &MarketplaceClient,
    provider_pda: &Pubkey,
    user: &Pubkey,
    balance: u64,
) -> Result<VerifyBalanceResult> {
    let provider = client
        .program
        .account::<marketplace::Provider>(*provider_pda)
        .context("Failed to fetch provider details")?;

    let region_filter = vec![
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8,
            vec![marketplace::MuAccountType::ProviderRegion as u8],
        )),
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8 + 1,
            provider_pda.to_bytes().to_vec(),
        )),
    ];

    let regions = client
        .program
        .accounts::<marketplace::ProviderRegion>(region_filter)
        .context("Failed to fetch provider regions")?;

    let stack_filter = vec![
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8,
            vec![marketplace::MuAccountType::Stack as u8],
        )),
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(8 + 1, user.to_bytes().to_vec())),
    ];

    let stacks = client
        .program
        .accounts::<marketplace::Stack>(stack_filter)
        .context("Failed to fetch user stacks")?;

    let regions_with_stacks = stacks
        .into_iter()
        .map(|s| s.1.region)
        .collect::<HashSet<_>>();

    let regions_below_minimum = regions
        .into_iter()
        .filter(|r| r.1.min_escrow_balance > balance)
        .map(|(pda, region)| RegionBalanceInfo {
            region,
            user_has_stacks: regions_with_stacks.contains(&pda),
        })
        .collect();

    Ok(VerifyBalanceResult {
        provider,
        regions_below_minimum,
    })
}
