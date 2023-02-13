use anyhow::Result;
use clap::Parser;

use crate::config::{Config, ConfigOverride};

pub mod escrow;
pub mod list;
pub mod provider;
pub mod sign_request;
pub mod stack;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
pub enum Command {
    /// Provider management. If you're a developer, this is not what you're looking for.
    Provider {
        #[command(subcommand)]
        sub_command: provider::Command,
    },

    List {
        #[command(subcommand)]
        sub_command: list::Command,
    },

    Escrow {
        #[command(subcommand)]
        sub_command: escrow::Command,
    },

    Stack {
        #[command(subcommand)]
        sub_command: stack::Command,
    },

    SignRequest(sign_request::Command),
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
    let config = Config::discover(args.cfg_override)?;
    match args.command {
        Command::Provider { sub_command } => provider::execute(config, sub_command),
        Command::List { sub_command } => list::execute(config, sub_command),
        Command::Escrow { sub_command } => escrow::execute(config, sub_command),
        Command::Stack { sub_command } => stack::execute(config, sub_command),
        Command::SignRequest(command) => sign_request::execute(config, command),
    }
}
