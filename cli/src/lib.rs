use crate::config::ConfigOverride;
use anyhow::Result;
use clap::{ArgGroup, Parser};
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

    /// Manage Regions
    Region {
        #[clap(subcommand)]
        subcmd: ProviderRegionCommand,
    },
}

//TODO: Add json or yaml support
#[derive(Debug, Parser)]
pub enum ProviderRegionCommand {
    /// Create a new region. Can only be run once.
    Create {
        #[clap(long, help = "Region name")]
        name: String,

        #[clap(long, help = "Provider name")]
        provider: String,

        #[clap(long, help = "MuDB price based on GB per month")]
        mudb_gb_month_price: f32,

        #[clap(long, help = "MuFunction price per (CPU+MEM)")] //TODO: what is the unit
        mufunction_cpu_mem_price: f32,

        #[clap(long, help = "MuGateway price per million requests")]
        mugateway_mreqs_price: f32,

        #[clap(long, help = "bandwidth price based on TB per month")]
        bandwidth_price: f32,
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
        ProviderCommand::Region { subcmd } => provider_region_create(cfg_override, subcmd),
    }
}

fn provider_create(cfg_override: &ConfigOverride, name: String) -> Result<(), anyhow::Error> {
    let cfg = Config::discover(cfg_override)?;
    let marketplace = marketplace::MarketplaceClient::new(cfg)?;

    marketplace.create_provider(name)
}

fn provider_region_create(
    cfg_override: &ConfigOverride,
    subcmd: ProviderRegionCommand,
) -> Result<(), anyhow::Error> {
    todo!()
}
