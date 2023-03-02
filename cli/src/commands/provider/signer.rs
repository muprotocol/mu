use anchor_client::solana_sdk::signature::read_keypair_file;
use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser};

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new authorized signer
    Create(CreateArgs),
    PrintKey(PrintKeyArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, help = "Keypair URI of the authorized signer wallet")]
    signer_keypair: String,

    #[arg(long)]
    signer_skip_seed_phrase_validation: bool,

    #[arg(long)]
    signer_confirm_key: bool,

    #[arg(long, help = "Region number for which to create the authorized signer")]
    region_num: u32,
}

#[derive(Args, Debug)]
pub struct PrintKeyArgs {
    #[arg(long)]
    keypair_file: String,
}

pub fn execute(config: Config, cmd: Command) -> Result<()> {
    match cmd {
        Command::Create(args) => create(config, args),
        Command::PrintKey(args) => print_key(args),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let provider_keypair = config.get_signer()?;

    let (signer_keypair, wallet_manager) = Config::read_keypair_from_url(
        Some(&args.signer_keypair),
        args.signer_skip_seed_phrase_validation,
        args.signer_confirm_key,
    )
    .context("Failed to read signer keypair")?;

    let result = marketplace_client::signer::create(
        &client,
        provider_keypair,
        signer_keypair,
        args.region_num,
    );

    drop(wallet_manager);

    result
}

fn print_key(args: PrintKeyArgs) -> Result<()> {
    let keypair = read_keypair_file(args.keypair_file)
        .map_err(|f| anyhow!("Unable to read keypair file: {f}"))?;
    println!("{}", keypair.to_base58_string());
    Ok(())
}
