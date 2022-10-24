use std::path::PathBuf;

use anchor_client::solana_sdk::signature::read_keypair_file;
use anyhow::{anyhow, Result};
use clap::{Args, Parser};

use crate::{
    config::{Config, ConfigOverride},
    marketplace::MarketplaceClient,
};

mod region;
mod signer;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new provider
    Create(CreateArgs),

    /// Manage Regions
    Region {
        #[clap(subcommand)]
        subcmd: region::Command,
    },

    /// Manage authorized signers
    Signer {
        #[clap(subcommand)]
        subcmd: signer::Command,
    },
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(short, long)]
    name: String,

    #[arg(short, long, help = "Provider keypair file")]
    provider_keypair: PathBuf,
}

// TODO: parse?! why not run or execute or sth?
pub fn parse(cfg_override: &ConfigOverride, subcmd: Command) -> Result<()> {
    match subcmd {
        Command::Create(args) => create(cfg_override, args),

        Command::Region { subcmd } => region::parse(cfg_override, subcmd),
        Command::Signer { subcmd } => signer::parse(cfg_override, subcmd),
    }
}

fn create(cfg_override: &ConfigOverride, args: CreateArgs) -> Result<()> {
    // TODO: this code needs to live somewhere where it won't have to be repeated for every command
    let cfg = Config::discover(cfg_override)?;
    let marketplace = MarketplaceClient::new(cfg).unwrap(); //TODO(ask Kaveh) // TODO: unwrap

    // TODO: I feel we can support all types of keypairs (not just files) if we're smart here.
    let provider_keypair = read_keypair_file(args.provider_keypair)
        .map_err(|e| anyhow!("can not read keypair: {}", e.to_string()))
        .unwrap(); //TODO

    marketplace.create_provider(args.name, provider_keypair)
}
