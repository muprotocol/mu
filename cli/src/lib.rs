use crate::config::ConfigOverride;
use anyhow::Result;
use clap::Parser;
use config::Config;

pub mod config;
mod marketplace;
mod path;

// Version of the docker image.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[clap(version = VERSION)]
pub struct Opts {
    #[clap(flatten)]
    pub cfg_override: ConfigOverride,
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Parser)]
pub enum Command {
    /// Commands related to providers.
    Provider {
        #[clap(subcommand)]
        subcmd: ProviderCommand,
    },
}

#[derive(Debug, Parser)]
pub enum ProviderCommand {
    /// Create a new provider. Can only be run once.
    Create {
        #[clap(short, long)]
        name: String,
    },
}

pub fn entry(opts: Opts) -> Result<()> {
    match opts.command {
        Command::Provider { subcmd } => provider(&opts.cfg_override, subcmd),
    }
}

fn provider(cfg_override: &ConfigOverride, subcmd: ProviderCommand) -> Result<()> {
    match subcmd {
        ProviderCommand::Create { name } => provider_create(cfg_override, name),
    }
}

fn provider_create(cfg_override: &ConfigOverride, name: String) -> Result<(), anyhow::Error> {
    let cfg = Config::discover(cfg_override)?;
    let marketplace = marketplace::MarketplaceClient::new(cfg)?;

    marketplace.create_provider(name)
}
