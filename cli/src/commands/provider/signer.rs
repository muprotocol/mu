use std::path::PathBuf;

use anchor_client::solana_sdk::{
    pubkey::Pubkey, signature::read_keypair_file, signer::Signer, system_program,
};
use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser};

use crate::config::Config;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new agent
    Create(CreateArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, help = "Agent keypair")]
    signer_keypair: PathBuf,

    #[arg(long, help = "Region number")]
    region_num: u32,
}

pub fn execute(config: Config, cmd: Command) -> Result<()> {
    match cmd {
        Command::Create(args) => create(config, args),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let (_, mu_state) = client.get_mu_state()?;

    let provider_keypair = config.get_signer()?;

    let provider_token_account =
        client.get_provider_token_account(provider_keypair.pubkey(), &mu_state);

    let provider_pda = client.get_provider_pda(provider_keypair.pubkey());

    // TODO: I feel we can support all types of keypairs (not just files) if we're smart here.
    // TODO: read solana cli sources to see how they handle the keypair URL.
    let signer_keypair = read_keypair_file(args.signer_keypair)
        .map_err(|e| anyhow!("Can't read keypair: {}", e.to_string()))?;

    let region_pda = client.get_region_pda(&provider_keypair.pubkey(), args.region_num);

    let (signer_pda, _) = Pubkey::find_program_address(
        &[b"authorized_signer", &region_pda.to_bytes()],
        &client.program.id(),
    );

    let accounts = marketplace::accounts::CreateAuthorizedUsageSigner {
        provider: provider_pda,
        region: region_pda,
        authorized_signer: signer_pda,
        owner: provider_keypair.pubkey(),
        system_program: system_program::id(),
    };

    client
        .program
        .request()
        .accounts(accounts)
        .args(marketplace::instruction::CreateAuthorizedUsageSigner {
            signer: signer_keypair.pubkey(),
            token_account: provider_token_account,
        })
        .signer(provider_keypair.as_ref())
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send authorized signer creation transaction")?;

    Ok(())
}
