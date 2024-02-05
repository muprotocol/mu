use anyhow::Result;
use clap::{Args, Parser};

use crate::{config::Config, pwr_client, token_utils::token_amount_to_ui_amount};

mod region;
mod signer;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new provider
    Create(CreateArgs),

    /// View information about creating providers
    Info,

    /// View provider status
    Status,

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
        Command::Info => info(config),
        Command::Status => print_status(config),

        Command::Region { sub_command } => region::execute(config, sub_command),
        Command::Signer { sub_command } => signer::execute(config, sub_command),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let provider_keypair = config.get_signer()?;
    pwr_client::provider::create(&client, provider_keypair, args.name)
}

fn info(config: Config) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let (_, mu_state) = client.get_mu_state()?;
    let mint = client.get_mint(&mu_state)?;
    println!(
        "Current provider deposit amount is {} $MU",
        token_amount_to_ui_amount(&mint, mu_state.provider_deposit)
    );

    Ok(())
}

fn print_status(config: Config) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let provider_keypair = config.get_signer()?;
    let pda = client.get_provider_pda(provider_keypair.pubkey());
    let provider = client.program.account::<marketplace::Provider>(pda)?;
    println!(
        "Provider {} is {}",
        provider.name,
        if provider.authorized {
            "authorized"
        } else {
            "not authorized"
        }
    );
    Ok(())
}
