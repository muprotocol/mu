use anyhow::Result;
use clap::Parser;

use crate::config::{Config, ConfigOverride};

pub mod provider;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
pub enum Command {
    /// Provider management. If you're a developer, this is not what you're looking for.
    Provider {
        #[command(subcommand)]
        sub_command: provider::Command,
    },
}

#[derive(Debug, Parser)]
#[clap(version = VERSION, about)]
pub struct Args {
    #[command(flatten)]
    pub cfg_override: ConfigOverride,
    #[command(subcommand)]
    pub command: Command,
}

pub fn execute(args: Args) -> Result<()> {
    let config = Config::discover(&args.cfg_override)?;
    match args.command {
        Command::Provider { sub_command } => provider::execute(config, sub_command),
    }
}
