use std::path::PathBuf;

use anchor_client::solana_sdk::signature::read_keypair_file;
use anyhow::{anyhow, Result};
use clap::{Args, Parser};

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new agent
    Create(CreateArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, help = "Agent keypair")]
    signer_keypair: PathBuf,

    #[arg(long, help = "Region number")]
    region_num: u32,
}

pub fn execute(config: Config, cmd: Command) -> Result<()> {
    match cmd {
        Command::Create(args) => create(config, args),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let provider_keypair = config.get_signer()?;

    // TODO: I feel we can support all types of keypairs (not just files) if we're smart here.
    // TODO: read solana cli sources to see how they handle the keypair URL.
    let signer_keypair = read_keypair_file(args.signer_keypair)
        .map_err(|e| anyhow!("Can't read keypair: {}", e.to_string()))?;

    let region_pda = client.get_region_pda(&provider_keypair.pubkey(), args.region_num);

    marketplace_client::signer::create(&client, provider_keypair, &signer_keypair, region_pda)
}
