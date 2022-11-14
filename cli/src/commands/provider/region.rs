use anchor_client::{
    solana_client::rpc_config::RpcSendTransactionConfig, solana_sdk::system_program,
};
use anyhow::{Context, Result};
use clap::{Args, Parser};

use crate::config::Config;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new region
    Create(CreateArgs),
}

//TODO: Add json or yaml support input string or file support
// TODO: actually, is it even a good idea to take this many args on the command line?
#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, help = "Region name")]
    name: String,

    #[arg(
        long,
        help = "Region number, must be unique across all regions for a provider"
    )]
    region_num: u32,

    #[arg(long, help = "Million DB reads price")]
    million_db_reads: u32,

    #[arg(long, help = "Million DB writes price")]
    million_db_writes: u32,

    #[arg(long, help = "DB gigabyte price per month")]
    db_gigabyte_months: u32,

    #[arg(long, help = "Billion function-mb-instructions price")]
    billion_function_mb_instructions: u32,

    #[arg(long, help = "Million gateway requests price")]
    million_gateway_requests: u32,

    #[arg(long, help = "Gigabyte gateway traffic price")]
    gigabytes_gateway_traffic: u32,
}

pub fn execute(config: Config, sub_command: Command) -> Result<()> {
    match sub_command {
        Command::Create(args) => create(config, args),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let provider_keypair = config.get_signer()?;

    let provider_pda = client.get_provider_pda(provider_keypair.pubkey());

    // TODO: validation

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

    client
        .program
        .request()
        .accounts(accounts)
        .args(marketplace::instruction::CreateRegion {
            region_num: args.region_num,
            name: args.name,
            zones: 1,
            rates,
        })
        .signer(provider_keypair.as_ref())
        .send_with_spinner_and_config(RpcSendTransactionConfig {
            // TODO: what's preflight and what's a preflight commitment?
            skip_preflight: cfg!(debug_assertions),
            ..Default::default()
        })
        .context("Failed to send region creation transaction")?;

    Ok(())
}
