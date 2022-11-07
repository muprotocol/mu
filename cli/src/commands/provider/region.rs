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

    #[arg(long, help = "MuDB price based on GB per month")]
    mudb_gb_month_price: u64,

    #[arg(long, help = "MuFunction price per (CPU+MEM)")] //TODO: what is the unit
    mufunction_cpu_mem_price: u64,

    #[arg(long, help = "MuGateway price per million requests")]
    mugateway_mreqs_price: u64,

    #[arg(long, help = "bandwidth price based on TB per month")]
    bandwidth_price: u64,
}

pub fn execute(config: Config, subcmd: Command) -> Result<()> {
    match subcmd {
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

    let rates = marketplace::ServiceUnits {
        mudb_gb_month: args.mudb_gb_month_price,
        mufunction_cpu_mem: args.mufunction_cpu_mem_price,
        bandwidth: args.bandwidth_price,
        gateway_mreqs: args.mugateway_mreqs_price,
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
