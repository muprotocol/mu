use anchor_client::{
    solana_client::rpc_filter::{Memcmp, RpcFilterType},
    solana_sdk::pubkey::Pubkey,
};
use anyhow::Result;
use clap::{arg, Args, Parser};

use crate::{config::Config, token_utils::token_amount_to_ui_amount};

#[derive(Debug, Parser)]
pub enum Command {
    /// List Providers
    Provider(ListProviderCommand),

    /// List Regions
    Region(ListRegionCommand),
}

pub fn execute(config: Config, cmd: Command) -> Result<()> {
    match cmd {
        Command::Provider(sub_command) => execute_list_provider(config, sub_command),
        Command::Region(sub_command) => execute_list_region(config, sub_command),
    }
}

#[derive(Debug, Args)]
pub struct ListProviderCommand {
    #[arg(
        long,
        help = "Perform a prefix search on developer names (case-sensitive)"
    )]
    name_prefix: Option<String>,
}

#[derive(Debug, Args)]
pub struct ListRegionCommand {
    #[arg(long, help = "The provider for which regions should be listed")]
    provider: Pubkey,
}

pub fn execute_list_provider(config: Config, cmd: ListProviderCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let mut filters = vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
        8 + 32,
        vec![1],
    ))];

    if let Some(name_prefix) = cmd.name_prefix {
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8 + 32 + 4, // 4 more bytes for the prefix length
            name_prefix.as_bytes().to_vec(),
        )));
    }

    let accounts = client.program.accounts::<marketplace::Provider>(filters)?;

    for account in accounts {
        println!("{}: {}", account.1.name, account.0);
    }

    Ok(())
}

pub fn execute_list_region(config: Config, cmd: ListRegionCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let (_, mu) = client.get_mu_state()?;
    let mint = client.get_mint(&mu)?;

    let filters = vec![RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
        8,
        cmd.provider.to_bytes().to_vec(),
    ))];

    let mut accounts = client
        .program
        .accounts::<marketplace::ProviderRegion>(filters)?;

    accounts.sort_by_key(|x| x.1.region_num);

    for account in accounts {
        println!("{}. {}:", account.1.region_num, account.1.name);
        println!("\tKey: {}", account.0);
        println!(
            "\tMinimum escrow balance: {}",
            token_amount_to_ui_amount(&mint, account.1.min_escrow_balance)
        );
        println!("\tBase URL: {}", account.1.base_url);
        println!("\tRates:");
        println!(
            "\t\t1000 billion CPU instructions, 1 megabyte of memory: {}",
            token_amount_to_ui_amount(&mint, account.1.rates.function_mb_tera_instructions)
        );
        println!(
            "\t\t1 GB of DB storage per month: {}",
            token_amount_to_ui_amount(&mint, account.1.rates.db_gigabyte_months)
        );
        println!(
            "\t\t1 million DB reads: {}",
            token_amount_to_ui_amount(&mint, account.1.rates.million_db_reads)
        );
        println!(
            "\t\t1 million DB writes: {}",
            token_amount_to_ui_amount(&mint, account.1.rates.million_db_writes)
        );
        println!(
            "\t\t1 million gateway requests: {}",
            token_amount_to_ui_amount(&mint, account.1.rates.million_gateway_requests)
        );
        println!(
            "\t\t1 GB of gateway traffic: {}",
            token_amount_to_ui_amount(&mint, account.1.rates.gigabytes_gateway_traffic)
        );
    }

    Ok(())
}
