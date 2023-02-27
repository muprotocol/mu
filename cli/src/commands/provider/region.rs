use anchor_client::solana_sdk::system_program;
use anyhow::{bail, Context, Result};
use clap::{Args, Parser};

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new region
    Create(CreateArgs),
}

//TODO: Add json or yaml support input string or file support
#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, help = "Region name")]
    name: String,

    #[arg(
        long,
        help = "Region number, must be unique across all regions for a provider"
    )]
    region_num: u32,

    #[arg(
        long,
        help = "The minimum amount of escrow balance a user must have so their stacks will be deployed"
    )]
    min_escrow_balance: f64,

    #[arg(long, help = "Billion function instructions and MB of RAM")]
    billion_function_mb_instructions: u64,

    #[arg(long, help = "Database GB per month")]
    db_gigabyte_months: u64,

    #[arg(long, help = "Million Database reads")]
    million_db_reads: u64,

    #[arg(long, help = "Million Database writes")]
    million_db_writes: u64,

    #[arg(long, help = "Million gateway requests")]
    million_gateway_requests: u64,

    #[arg(long, help = "Gateway GB traffic")]
    gigabytes_gateway_traffic: u64,
}

pub fn execute(config: Config, sub_command: Command) -> Result<()> {
    match sub_command {
        Command::Create(args) => create(config, args),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let (_, state) = client.get_mu_state()?;
    let mint = client.get_mint(&state)?;
    let min_escrow_balance =
        crate::token_utils::ui_amount_to_token_amount(&mint, args.min_escrow_balance);

    let provider_keypair = config.get_signer()?;

    let provider_pda = client.get_provider_pda(provider_keypair.pubkey());
    let provider = client
        .program
        .account::<marketplace::Provider>(provider_pda)
        .context("Failed to fetch provider account")?;

    if !provider.authorized {
        bail!("Provider is not authorized, can't create regions yet");
    }

    let region_pda = client.get_region_pda(&provider_keypair.pubkey(), args.region_num);

    let accounts = marketplace::accounts::CreateRegion {
        provider: provider_pda,
        region: region_pda,
        owner: provider_keypair.pubkey(),
        system_program: system_program::id(),
    };

    let rates = marketplace::ServiceRates {
        billion_function_mb_instructions: args.billion_function_mb_instructions,
        db_gigabyte_months: args.db_gigabyte_months,
        million_db_reads: args.million_db_reads,
        million_db_writes: args.million_db_writes,
        million_gateway_requests: args.million_gateway_requests,
        gigabytes_gateway_traffic: args.gigabytes_gateway_traffic,
    };

    let instruction = marketplace::instruction::CreateRegion {
        region_num: args.region_num,
        name: args.name,
        min_escrow_balance,
        rates,
    };

    marketplace_client::region::create(&client, accounts, instruction, provider_keypair)
}
