use anchor_client::{
    anchor_lang::AccountDeserialize,
    solana_client::{rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig},
    solana_sdk::{account::ReadableAccount, pubkey::Pubkey, system_program},
};
use anyhow::{bail, Context, Result};
use clap::{Args, Parser};
use solana_account_decoder::parse_token::{self, TokenAccountType};

use crate::config::Config;

#[derive(Debug, Parser)]
pub enum Command {
    /// Create a new region
    Create(CreateArgs),
}

//TODO: Add json or yaml support input string or file support
// TODO: actually, is it even a good idea to take this many args on the command line?
#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long, help = "Region name")]
    name: String,

    #[arg(
        long,
        help = "Region number, must be unique across all regions for a provider"
    )]
    region_num: u32,

    #[arg(
        long,
        help = "The minimum amount of escrow balance a user must have so their stacks will be deployed"
    )]
    min_escrow_balance: f64,

    #[arg(long, help = "Billion function instructions and MB of RAM")]
    billion_function_mb_instructions: u64,

    #[arg(long, help = "Database GB per month")]
    db_gigabyte_months: u64,

    #[arg(long, help = "Million Database reads")]
    million_db_reads: u64,

    #[arg(long, help = "Million Database writes")]
    million_db_writes: u64,

    #[arg(long, help = "Million gateway requests")]
    million_gateway_requests: u64,

    #[arg(long, help = "Gateway GB traffic")]
    gigabytes_gateway_traffic: u64,
}

pub fn execute(config: Config, sub_command: Command) -> Result<()> {
    match sub_command {
        Command::Create(args) => create(config, args),
    }
}

fn create(config: Config, args: CreateArgs) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let token_decimals = get_token_decimals(&client.program.rpc())?;
    let min_escrow_balance = ui_amount_to_token_amount(args.min_escrow_balance, token_decimals);

    let provider_keypair = config.get_signer()?;

    let provider_pda = client.get_provider_pda(provider_keypair.pubkey());

    // TODO: validation

    let region_pda = client.get_region_pda(&provider_keypair.pubkey(), args.region_num);

    let accounts = marketplace::accounts::CreateRegion {
        provider: provider_pda,
        region: region_pda,
        owner: provider_keypair.pubkey(),
        system_program: system_program::id(),
    };

    let rates = marketplace::ServiceRates {
        billion_function_mb_instructions: args.billion_function_mb_instructions,
        db_gigabyte_months: args.db_gigabyte_months,
        million_db_reads: args.million_db_reads,
        million_db_writes: args.million_db_writes,
        million_gateway_requests: args.million_gateway_requests,
        gigabytes_gateway_traffic: args.gigabytes_gateway_traffic,
    };

    client
        .program
        .request()
        .accounts(accounts)
        .args(marketplace::instruction::CreateRegion {
            region_num: args.region_num,
            name: args.name,
            min_escrow_balance,
            rates,
        })
        .signer(provider_keypair.as_ref())
        .send_with_spinner_and_config(RpcSendTransactionConfig {
            // TODO: what's preflight and what's a preflight commitment?
            skip_preflight: cfg!(debug_assertions),
            ..Default::default()
        })
        .context("Failed to send region creation transaction")?;

    Ok(())
}

fn get_token_decimals(rpc_client: &RpcClient) -> Result<u8> {
    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &marketplace::id());
    let state = rpc_client
        .get_account(&state_pda)
        .context("Failed to fetch mu state from Solana")?;
    let state = marketplace::MuState::try_deserialize(&mut state.data())
        .context("Failed to read mu state from Solana")?;

    let mint_address = state.mint;
    let mint = rpc_client
        .get_account(&mint_address)
        .context("Failed to fetch $MU mint from Solana")?;
    let mint = parse_token::parse_token(mint.data(), None)
        .context("Failed to read $MU mint from Solana")?;

    if let TokenAccountType::Mint(mint) = mint {
        Ok(mint.decimals)
    } else {
        bail!("Expected $MU mint to be a mint account");
    }
}

fn ui_amount_to_token_amount(ui_amount: f64, decimals: u8) -> u64 {
    let exp = 10u64.pow(decimals as u32) as f64;
    (ui_amount * exp).round() as u64
}
