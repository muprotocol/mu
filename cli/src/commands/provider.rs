use std::path::PathBuf;

use anchor_client::{
    solana_client::rpc_config::RpcSendTransactionConfig,
    solana_sdk::{
        pubkey::Pubkey, signature::read_keypair_file, signer::Signer, system_program, sysvar,
    },
};
use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser};

use crate::config::Config;

mod region;
mod signer;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new provider
    Create(CreateArgs),

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

    #[arg(long, help = "Provider keypair file")]
    provider_keypair: PathBuf,
}

// TODO: parse?! why not run or execute or sth?
pub fn execute(config: Config, sub_command: Command) -> Result<()> {
    match sub_command {
        Command::Create(args) => create(config, args),

        Command::Region { sub_command } => region::execute(config, sub_command),
        Command::Signer { sub_command } => signer::execute(config, sub_command),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;

    // TODO: I feel we can support all types of keypairs (not just files) if we're smart here.
    // TODO: shouldn't this be just the payer wallet?
    let provider_keypair = read_keypair_file(args.provider_keypair)
        .map_err(|e| anyhow!("Can't read keypair: {}", e.to_string()))?;

    let (state_pda, mu_state) = client.get_mu_state()?;

    let (deposit_pda, _) = Pubkey::find_program_address(&[b"deposit"], &client.program.id());
    let provider_pda = client.get_provider_pda(provider_keypair.pubkey());

    // TODO: we need to double-check all error conditions and generate user-readable error messages.
    // there is no backend server to return cute messages, only the deep, dark bowels of the blockchain.
    let provider_token_account =
        client.get_provider_token_account(provider_keypair.pubkey(), &mu_state);

    let accounts = marketplace::accounts::CreateProvider {
        state: state_pda,
        provider: provider_pda,
        deposit_token: deposit_pda,
        owner: provider_keypair.pubkey(),
        owner_token: provider_token_account,
        system_program: system_program::id(),
        token_program: spl_token::id(),
        rent: sysvar::rent::id(),
    };

    client
        .program
        .request()
        .accounts(accounts)
        .args(marketplace::instruction::CreateProvider { name: args.name })
        .signer(&provider_keypair)
        .send_with_spinner_and_config(RpcSendTransactionConfig {
            // TODO: what's preflight and what's a preflight commitment?
            skip_preflight: cfg!(debug_assertions),
            ..Default::default()
        })
        .context("Failed to send provider creation transaction")?;

    Ok(())
}
