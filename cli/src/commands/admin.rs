use anchor_client::{
    solana_client::rpc_filter::{Memcmp, RpcFilterType},
    solana_sdk::{program_pack::Pack, pubkey::Pubkey},
};
use anyhow::{Context, Result};
use clap::Parser;
use spl_token::state::Mint;

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    Initialize(InitializeCommand),
    UpdateDeposit(UpdateDepositCommand),
    CreateProviderAuthorizer(CreateAuthorizerCommand),
    ListUnauthorizedProviders,
    AuthorizeProvider(AuthorizeProviderCommand),
}

#[derive(Debug, Parser)]
pub struct InitializeCommand {
    #[arg(long)]
    token_mint: Pubkey,

    #[arg(long)]
    commission_rate_micros: u32,

    #[arg(long)]
    provider_deposit: f64,
}

#[derive(Debug, Parser)]
pub struct UpdateDepositCommand {
    deposit: f64,
}

#[derive(Debug, Parser)]
pub struct CreateAuthorizerCommand {
    authorizer_keypair: String,
}

#[derive(Debug, Parser)]
pub struct AuthorizeProviderCommand {
    provider: Pubkey,
}

pub fn execute(config: Config, command: Command) -> Result<()> {
    match command {
        Command::Initialize(cmd) => execute_initialize(config, cmd),
        Command::UpdateDeposit(cmd) => execute_update_deposit(config, cmd),
        Command::CreateProviderAuthorizer(cmd) => execute_create_provider_authorizer(config, cmd),
        Command::ListUnauthorizedProviders => execute_list_unauthorized_providers(config),
        Command::AuthorizeProvider(cmd) => execute_authorize_provider(config, cmd),
    }
}

fn execute_initialize(config: Config, command: InitializeCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let signer = config.get_signer()?;
    let mint_account = client.program.rpc().get_account(&command.token_mint)?;
    let mint =
        <Mint as Pack>::unpack(&mint_account.data).context("Failed to parse mint account data")?;
    marketplace_client::admin::initialize(
        &client,
        signer.as_ref(),
        command.token_mint,
        command.commission_rate_micros,
        crate::token_utils::ui_amount_to_token_amount(&mint, command.provider_deposit),
    )
}

fn execute_update_deposit(config: Config, command: UpdateDepositCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let signer = config.get_signer()?;
    let (_, mu_state) = client.get_mu_state()?;
    let mint = client.get_mint(&mu_state)?;
    marketplace_client::admin::update_deposit(
        &client,
        signer.as_ref(),
        crate::token_utils::ui_amount_to_token_amount(&mint, command.deposit),
    )
}

fn execute_create_provider_authorizer(
    config: Config,
    command: CreateAuthorizerCommand,
) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let signer = config.get_signer()?;
    let (authorizer, _) =
        Config::read_keypair_from_url(Some(&command.authorizer_keypair), false, false)?;
    marketplace_client::admin::create_provider_authorizer(
        &client,
        signer.as_ref(),
        authorizer.as_ref(),
    )
}

fn execute_list_unauthorized_providers(config: Config) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let providers =
        client
            .program
            .accounts::<marketplace::Provider>(vec![RpcFilterType::Memcmp(
                Memcmp::new_raw_bytes(8 + 32, vec![0u8]),
            )])?;

    for provider in providers {
        println!("{}: {}", provider.1.owner, provider.1.name);
    }

    Ok(())
}

fn execute_authorize_provider(config: Config, command: AuthorizeProviderCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let signer = config.get_signer()?;
    marketplace_client::admin::authorize_provider(&client, signer.as_ref(), command.provider)
}
