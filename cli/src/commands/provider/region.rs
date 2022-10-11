use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use clap::{Args, Parser};

use crate::config::ConfigOverride;

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

    #[arg(long, help = "Provider Pubkey")]
    provider: Pubkey,

    #[arg(long, help = "MuDB price based on GB per month")]
    mudb_gb_month_price: f32,

    #[arg(long, help = "MuFunction price per (CPU+MEM)")] //TODO: what is the unit
    mufunction_cpu_mem_price: f32,

    #[arg(long, help = "MuGateway price per million requests")]
    mugateway_mreqs_price: f32,

    #[arg(long, help = "bandwidth price based on TB per month")]
    bandwidth_price: f32,
}

pub fn parse(cfg_override: &ConfigOverride, subcmd: Command) -> Result<()> {
    match subcmd {
        Command::Create(args) => create(cfg_override, args),
    }
}

fn create(_cfg_override: &ConfigOverride, _args: CreateArgs) -> Result<()> {
    todo!()
}
