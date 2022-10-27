use std::path::PathBuf;

use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use clap::{Args, Parser};

use crate::config::Config;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new agent
    Create(CreateArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, help = "Agent name")]
    name: String,

    #[arg(long, help = "Provider Pubkey")]
    provider: Pubkey,

    #[arg(long, help = "Agent keypair")] //TODO
    keypair: PathBuf,
}

pub fn execute(config: Config, subcmd: Command) -> Result<()> {
    match subcmd {
        Command::Create(args) => create(config, args),
    }
}

fn create(config: Config, _args: CreateArgs) -> Result<()> {
    todo!()
}
