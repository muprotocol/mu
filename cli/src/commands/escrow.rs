use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use clap::{Args, Parser};
use spl_associated_token_account::get_associated_token_address;

use crate::{
    config::Config,
    marketplace_client::{
        self,
        escrow::{get_escrow_balance, get_regions_where_balance_is_below_minimum},
    },
    token_utils::ui_amount_to_token_amount,
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

    marketplace_client::escrow::create(&client, user_wallet, cmd.provider)
}

pub fn execute_recharge(config: Config, cmd: RechargeEscrowCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let (_, mu_state) = client.get_mu_state()?;
    let mint = client.get_mint(&mu_state)?;

    let recharge_amount = ui_amount_to_token_amount(&mint, cmd.recharge_amount);

    let user_wallet = config.get_signer()?;
    let user_token_account = get_associated_token_address(&user_wallet.pubkey(), &mu_state.mint);

    marketplace_client::escrow::recharge(
        &client,
        user_wallet,
        recharge_amount,
        &cmd.provider,
        &user_token_account,
    )
}

pub fn execute_withdraw(config: Config, cmd: WithdrawEscrowCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let user_wallet = config.get_signer()?;

    marketplace_client::escrow::withdraw(
        &client,
        user_wallet,
        &cmd.provider,
        cmd.withdraw_amount,
        cmd.withdraw_to,
        cmd.force,
    )
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
