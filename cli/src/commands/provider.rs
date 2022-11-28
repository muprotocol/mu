use anyhow::Result;
use clap::{Args, Parser};

use crate::config::Config;

mod region;
mod signer;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new provider
    Create(CreateArgs),

    /// Manage Regions
    Region {
        #[clap(subcommand)]
        sub_command: region::Command,
    },

    /// Manage authorized signers
    Signer {
        #[clap(subcommand)]
        sub_command: signer::Command,
    },
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(short, long)]
    name: String,
}

pub fn execute(config: Config, sub_command: Command) -> Result<()> {
    match sub_command {
        Command::Create(args) => create(config, args),

        Command::Region { sub_command } => region::execute(config, sub_command),
        Command::Signer { sub_command } => signer::execute(config, sub_command),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let provider_keypair = config.get_signer()?;
    client.create_provider(provider_keypair, args.name)
}
