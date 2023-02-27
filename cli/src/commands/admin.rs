use anchor_client::{
    solana_client::rpc_filter::{Memcmp, RpcFilterType},
    solana_sdk::pubkey::Pubkey,
};
use anyhow::Result;
use clap::Parser;

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    Initialize(InitializeCommand),
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
        Command::CreateProviderAuthorizer(cmd) => execute_create_provider_authorizer(config, cmd),
        Command::ListUnauthorizedProviders => execute_list_unauthorized_providers(config),
        Command::AuthorizeProvider(cmd) => execute_authorize_provider(config, cmd),
    }
}

fn execute_initialize(config: Config, command: InitializeCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let signer = config.get_signer()?;
    marketplace_client::admin::initialize(
        &client,
        signer.as_ref(),
        command.token_mint,
        command.commission_rate_micros,
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
