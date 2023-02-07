use anyhow::{Context, Result};
use clap::{Args, Parser};

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new authorized signer
    Create(CreateArgs),
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

pub fn execute(config: Config, cmd: Command) -> Result<()> {
    match cmd {
        Command::Create(args) => create(config, args),
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

    let region_pda = client.get_region_pda(&provider_keypair.pubkey(), args.region_num);

    let result =
        marketplace_client::signer::create(&client, provider_keypair, signer_keypair, region_pda);

    drop(wallet_manager);

    result
}
