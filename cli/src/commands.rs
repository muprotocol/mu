use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use clap::{Args, Parser};

use crate::{
    config::{Config, ConfigOverride},
    marketplace_client,
};

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

    /// Deploy the project
    Deploy(DeployStackCommand),
}

#[derive(Debug, Args)]
pub struct DeployStackCommand {
    #[arg(long, short)]
    /// Seed numbers are used to distinguish stacks deployed to the same region.
    /// The seed can be thought of as an ID, which is used again when updating
    /// the same stack.
    seed: u64,

    #[arg(long)]
    /// The region to deploy to.
    region: Pubkey,

    #[arg(long)]
    /// If specified, only deploy the stack if it doesn't already exist
    init: bool,

    #[arg(long)]
    /// If specified, only update the stack if a previous version already exists
    update: bool,
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
        Command::Deploy(sub_command) => execute_deploy(config, sub_command),

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

pub fn execute_deploy(config: Config, cmd: DeployStackCommand) -> Result<()> {
    let (mu_manifest, project_root) = crate::mu_manifest::read_manifest()?;
    mu_manifest.build_all(crate::mu_manifest::BuildMode::Release, &project_root)?;

    let marketplace_client = config.build_marketplace_client()?;
    let user_wallet = config.get_signer()?;

    let region_base_url =
        marketplace_client::region::get_base_url(&marketplace_client, cmd.region)?;

    let region_api_client = api_common::client::ApiClient::new(region_base_url);

    println!("Signing function upload request ...");
    let stack = mu_manifest.generate_stack_manifest_for_publish(
        |p| region_api_client.upload_function(std::path::PathBuf::from(p), user_wallet.clone()),
        &project_root,
    )?;

    let deploy_mode = marketplace_client::stack::get_deploy_mode(cmd.init, cmd.update)?;

    marketplace_client::stack::deploy(
        &marketplace_client,
        user_wallet,
        &cmd.region,
        stack,
        cmd.seed,
        deploy_mode,
    )
}
