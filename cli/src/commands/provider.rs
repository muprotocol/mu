use std::path::PathBuf;

use anchor_client::solana_sdk::signature::read_keypair_file;
use anyhow::{anyhow, Result};
use clap::{Args, Parser};

use crate::{
    config::{Config, ConfigOverride},
    marketplace::MarketplaceClient,
};

mod agent;
mod region;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new provider
    Create(CreateArgs),

    /// Manage Regions
    Region {
        #[clap(subcommand)]
        subcmd: region::Command,
    },

    /// Manage Agents
    Agent {
        #[clap(subcommand)]
        subcmd: agent::Command,
    },
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(short, long)]
    name: String,

    #[arg(short, long, help = "Provider keypair file")]
    provider_keypair: PathBuf,
}

pub fn parse(cfg_override: &ConfigOverride, subcmd: Command) -> Result<()> {
    match subcmd {
        Command::Create(args) => create(cfg_override, args),

        Command::Region { subcmd } => region::parse(cfg_override, subcmd),
        Command::Agent { subcmd } => agent::parse(cfg_override, subcmd),
    }
}

fn create(cfg_override: &ConfigOverride, args: CreateArgs) -> Result<()> {
    let cfg = Config::discover(cfg_override)?;
    let marketplace = MarketplaceClient::new(cfg)?;
    let provider_keypair = read_keypair_file(args.provider_keypair)
        .map_err(|e| anyhow!("can not read keypair: {}", e.to_string()))?;

    marketplace.create_provider(args.name, provider_keypair)
}
