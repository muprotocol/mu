use anyhow::Result;
use clap::Parser;

use crate::config::{Config, ConfigOverride};

pub mod dev_env;
pub mod escrow;
pub mod list;
pub mod provider;
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

    /// Initialize a new mu project
    Init(dev_env::InitCommand),

    /// Build mu project
    Build(dev_env::BuildCommand),

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

        Command::Init(sub_command) => dev_env::execute_init(sub_command),
        Command::Build(sub_command) => dev_env::execute_build(sub_command),
        Command::Run(sub_command) => dev_env::execute_run(sub_command),
    }
}
