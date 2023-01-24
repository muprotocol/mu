use std::{collections::HashSet, process::exit};

//TODO: validations
use anchor_client::{
    solana_client::{
        client_error::{ClientError, ClientErrorKind},
        rpc_filter::{Memcmp, RpcFilterType},
        rpc_request::RpcError,
    },
    solana_sdk::{pubkey::Pubkey, system_program, sysvar::rent},
};
use anyhow::{bail, Context, Result};
use clap::{Args, Parser};
use marketplace::{Provider, ProviderRegion};
use solana_account_decoder::parse_token::UiTokenAmount;
use spl_associated_token_account::get_associated_token_address;

use crate::{
    config::Config,
    marketplace_client::MarketplaceClient,
    token_utils::{token_amount_to_ui_amount, ui_amount_to_token_amount},
};

#[derive(Debug, Parser)]
pub enum Command {
    Create(CreateEscrowCommand),
    Recharge(RechargeEscrowCommand),
    Withdraw(WithdrawEscrowCommand),
    View(ViewEscrowCommand),
}

#[derive(Debug, Args)]
pub struct CreateEscrowCommand {
    /// The provider for which to create an escrow account. Note that escrow accounts are per-provider.
    provider: Pubkey,
}

#[derive(Debug, Args)]
pub struct RechargeEscrowCommand {
    /// The provider for which to recharge the escrow account. Note that escrow accounts are per-provider.
    provider: Pubkey,

    /// The amount to charge the escrow account. This amount will be transferred from the $MU
    /// token account associated with the user wallet to the escrow account.
    #[arg(long = "amount")]
    recharge_amount: f64,
}

#[derive(Debug, Args)]
pub struct WithdrawEscrowCommand {
    /// The provider for which to recharge the escrow account. Note that escrow accounts are per-provider.
    provider: Pubkey,

    /// The amount to withdraw from the escrow account.
    #[arg(long = "amount")]
    withdraw_amount: f64,

    /// The $MU account to transfer the withdrawn amount to. If left out, the amount will be transferred
    /// to the token account associated with the user wallet.
    withdraw_to: Option<Pubkey>,

    /// If specified, amount will be withdrawn even if it would result in the escrow balance going below
    /// minimum for regions where stacks are deployed.
    #[arg(short, long)]
    force: bool,
}

#[derive(Debug, Args)]
pub struct ViewEscrowCommand {
    /// The provider for which to view the escrow account. Note that escrow accounts are per-provider.
    provider: Pubkey,
}

pub fn execute(config: Config, cmd: Command) -> Result<()> {
    match cmd {
        Command::Create(sub_command) => execute_create(config, sub_command),
        Command::Recharge(sub_command) => execute_recharge(config, sub_command),
        Command::Withdraw(sub_command) => execute_withdraw(config, sub_command),
        Command::View(sub_command) => execute_view(config, sub_command),
    }
}

pub fn execute_create(config: Config, cmd: CreateEscrowCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let user_wallet = config.get_signer()?;

    let (mu_state_pda, mu_state) = client.get_mu_state()?;

    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), &cmd.provider);

    let accounts = marketplace::accounts::CreateProviderEscrowAccount {
        state: mu_state_pda,
        mint: mu_state.mint,
        escrow_account: escrow_pda,
        provider: cmd.provider,
        user: user_wallet.pubkey(),
        system_program: system_program::id(),
        token_program: spl_token::id(),
        rent: rent::id(),
    };

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

pub fn execute_recharge(config: Config, cmd: RechargeEscrowCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let (_, mu_state) = client.get_mu_state()?;
    let mint = client.get_mint(&mu_state)?;

    let recharge_amount = ui_amount_to_token_amount(&mint, cmd.recharge_amount);

    let user_wallet = config.get_signer()?;
    let user_token_account = get_associated_token_address(&user_wallet.pubkey(), &mu_state.mint);

    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), &cmd.provider);

    client
        .program
        .request()
        .instruction(spl_token::instruction::transfer(
            &spl_token::id(),
            &user_token_account,
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

pub fn execute_withdraw(config: Config, cmd: WithdrawEscrowCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let (state_pda, mu_state) = client.get_mu_state()?;
    let mint = client.get_mint(&mu_state)?;

    let withdraw_amount = ui_amount_to_token_amount(&mint, cmd.withdraw_amount);

    let user_wallet = config.get_signer()?;

    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), &cmd.provider);
    let token_account = get_escrow_balance(&client, &escrow_pda)?;
    let balance: u64 = token_account.amount.parse().unwrap();

    if balance < withdraw_amount {
        println!(
            "Escrow account balance is less than specified amount. Balance is: {}",
            token_amount_to_ui_amount(&mint, balance)
        );
        exit(-1);
    }

    let verify_result = get_regions_where_balance_is_below_minimum(
        &client,
        &cmd.provider,
        &user_wallet.pubkey(),
        balance - withdraw_amount,
    )?;

    let regions_below_minimum_with_stacks = verify_result
        .regions_below_minimum
        .iter()
        .filter(|x| x.user_has_stacks)
        .collect::<Vec<_>>();

    if !regions_below_minimum_with_stacks.is_empty() && !cmd.force {
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

    let withdraw_to = cmd
        .withdraw_to
        .unwrap_or_else(|| get_associated_token_address(&user_wallet.pubkey(), &mu_state.mint));

    let accounts = marketplace::accounts::WithdrawEscrow {
        state: state_pda,
        escrow_account: escrow_pda,
        user: user_wallet.pubkey(),
        provider: cmd.provider,
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

pub fn execute_view(config: Config, cmd: ViewEscrowCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let user_wallet = config.get_signer()?;

    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), &cmd.provider);
    let token_account = get_escrow_balance(&client, &escrow_pda)?;
    let balance: u64 = token_account.amount.parse().unwrap();

    let verify_balance = get_regions_where_balance_is_below_minimum(
        &client,
        &cmd.provider,
        &user_wallet.pubkey(),
        balance,
    )?;

    println!(
        "Escrow account for provider '{}':",
        verify_balance.provider.name
    );
    println!("\tAccount key: {}", escrow_pda);
    println!("\tBalance: {}", token_account.ui_amount_string);
    println!();

    if !verify_balance.regions_below_minimum.is_empty() {
        println!("Escrow balance is below minimum for following regions:");
        for region in &verify_balance.regions_below_minimum {
            print!("\t{}", region.region.name);
            if region.user_has_stacks {
                print!(" <-- WARNING! This region contains active stack deployments");
            }
            println!();
        }
        println!();
    }

    println!("Note: to recharge, you can use `mu escrow recharge` or make direct token transfers to this account.");

    Ok(())
}

fn get_escrow_balance(client: &MarketplaceClient, escrow_pda: &Pubkey) -> Result<UiTokenAmount> {
    match client.program.rpc().get_token_account_balance(escrow_pda) {
        Ok(token_account) => Ok(token_account),
        Err(ClientError {
            kind: ClientErrorKind::RpcError(RpcError::RpcResponseError { code: -32602, .. }),
            ..
        }) => bail!("Escrow account does not exist"),
        Err(f) => Err(f).context("Failed to fetch escrow account balance"),
    }
}

struct RegionBalanceInfo {
    region: ProviderRegion,
    user_has_stacks: bool,
}

struct VerifyBalanceResult {
    provider: Provider,
    regions_below_minimum: Vec<RegionBalanceInfo>,
}

fn get_regions_where_balance_is_below_minimum(
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
