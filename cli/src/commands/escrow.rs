//TODO: validations
use anchor_client::{
    solana_client::{
        client_error::{ClientError, ClientErrorKind},
        rpc_request::RpcError,
    },
    solana_sdk::{program_pack::Pack, pubkey::Pubkey},
};
use anyhow::{Context, Result};
use clap::{Args, Parser};
use spl_associated_token_account::get_associated_token_address;
use spl_token::state::Mint;

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    Create(CreateEscrowCommand),
    Recharge(RechargeEscrowCommand),
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
    /// token account associated with the user wallet to the newly created escrow account.
    recharge_amount: f64,
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

    let mint_account = client.program.rpc().get_account(&mu_state.mint)?;
    let mint = <Mint as Pack>::unpack(&mint_account.data)?;
    let recharge_amount =
        (cmd.recharge_amount * 10u64.pow(mint.decimals as u32) as f64).floor() as u64;

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

pub fn execute_view(config: Config, cmd: ViewEscrowCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let user_wallet = config.get_signer()?;

    let escrow_pda = client.get_escrow_pda(&user_wallet.pubkey(), &cmd.provider);

    let provider = client
        .program
        .account::<marketplace::Provider>(cmd.provider)
        .context("Failed to fetch provider details")?;

    match client.program.rpc().get_token_account_balance(&escrow_pda) {
        Ok(token_account) => {
            println!("Escrow account for provider '{}':", provider.name);
            println!("\tAccount key: {}", escrow_pda);
            println!("\tBalance: {}", token_account.ui_amount_string);
            println!();
            println!("Note: to recharge, you can use `mu escrow recharge` or make direct token transfers to this account.");
        }
        Err(ClientError {
            kind: ClientErrorKind::RpcError(RpcError::RpcResponseError { code: -32602, .. }),
            ..
        }) => println!("Escrow account does not exist"),
        Err(f) => return Err(f).context("Failed to fetch token account balance"),
    }

    Ok(())
}
