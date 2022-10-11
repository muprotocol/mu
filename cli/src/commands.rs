use anyhow::Result;
use clap::Parser;

use crate::config::ConfigOverride;

pub mod provider;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
pub enum Command {
    /// Commands related to providers.
    Provider {
        #[command(subcommand)]
        subcmd: provider::Command,
    },
}

#[derive(Debug, Parser)]
#[clap(version = VERSION, about)]
pub struct Opts {
    #[command(flatten)]
    pub cfg_override: ConfigOverride,
    #[command(subcommand)]
    pub command: Command,
}

pub fn entry(opts: Opts) -> Result<()> {
    match opts.command {
        Command::Provider { subcmd } => provider::parse(&opts.cfg_override, subcmd),
    }
}
