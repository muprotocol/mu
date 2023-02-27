use anyhow::Result;
use clap::Parser;

use crate::config::{Config, ConfigOverride};

pub mod escrow;
pub mod list;
pub mod provider;
pub mod request_signer;
pub mod stack;

#[cfg(feature = "admin")]
pub mod admin;

#[cfg(feature = "dev-env")]
pub mod dev_env;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
pub enum Command {
    /// Provider management. If you're a developer, this is not what you're looking for.
    Provider {
        #[command(subcommand)]
        sub_command: provider::Command,
    },

    /// List available providers and regions
    List {
        #[command(subcommand)]
        sub_command: list::Command,
    },

    /// Escrow account management
    Escrow {
        #[command(subcommand)]
        sub_command: escrow::Command,
    },

    /// Stack management
    Stack {
        #[command(subcommand)]
        sub_command: stack::Command,
    },

    /// API request signer management and request signing
    RequestSigner {
        #[command(subcommand)]
        sub_command: request_signer::Command,
    },

    #[cfg(feature = "admin")]
    Admin {
        #[command(subcommand)]
        sub_command: admin::Command,
    },

    #[cfg(feature = "dev-env")]
    /// Initialize a new mu project
    Init(dev_env::InitCommand),

    #[cfg(feature = "dev-env")]
    /// Build mu project
    Build(dev_env::BuildCommand),

    #[cfg(feature = "dev-env")]
    /// Run mu project
    Run(dev_env::RunCommand),
}

#[derive(Debug, Parser)]
#[clap(version = VERSION, about)]
pub struct Arguments {
    #[command(flatten)]
    pub cfg_override: ConfigOverride,
    #[command(subcommand)]
    pub command: Command,
}

pub fn execute(args: Arguments) -> Result<()> {
    let config = Config::discover(args.cfg_override)?;
    match args.command {
        Command::Provider { sub_command } => provider::execute(config, sub_command),
        Command::List { sub_command } => list::execute(config, sub_command),
        Command::Escrow { sub_command } => escrow::execute(config, sub_command),
        Command::Stack { sub_command } => stack::execute(config, sub_command),
        Command::RequestSigner { sub_command } => request_signer::execute(config, sub_command),

        #[cfg(feature = "admin")]
        Command::Admin { sub_command } => admin::execute(config, sub_command),

        #[cfg(feature = "dev-env")]
        Command::Init(sub_command) => dev_env::execute_init(sub_command),
        #[cfg(feature = "dev-env")]
        Command::Build(sub_command) => dev_env::execute_build(sub_command),
        #[cfg(feature = "dev-env")]
        Command::Run(sub_command) => dev_env::execute_run(sub_command),
    }
}
