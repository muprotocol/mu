use anchor_client::{
    solana_client::rpc_filter::{Memcmp, MemcmpEncodedBytes, MemcmpEncoding, RpcFilterType},
    solana_sdk::pubkey::Pubkey,
};
use anyhow::Result;
use clap::{arg, Args, Parser};

use crate::config::Config;

#[derive(Debug, Parser)]
pub enum Command {
    /// Manage Regions
    Provider(ListProviderCommand),

    /// Manage authorized signers
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

    let mut filters = vec![RpcFilterType::Memcmp(Memcmp {
        offset: 8,
        bytes: MemcmpEncodedBytes::Bytes(vec![marketplace::MuAccountType::Provider as u8]),
        encoding: Some(MemcmpEncoding::Binary),
    })];

    if let Some(name_prefix) = cmd.name_prefix {
        filters.push(RpcFilterType::Memcmp(Memcmp {
            offset: 8 + 1 + 32 + 4, // 4 more bytes for the prefix length
            bytes: MemcmpEncodedBytes::Bytes(name_prefix.as_bytes().to_vec()),
            encoding: Some(MemcmpEncoding::Binary),
        }));
    }

    let accounts = client.program.accounts::<marketplace::Provider>(filters)?;

    for account in accounts {
        println!("{}: {}", account.1.name, account.0);
    }

    Ok(())
}

pub fn execute_list_region(config: Config, cmd: ListRegionCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;

    let filters = vec![
        RpcFilterType::Memcmp(Memcmp {
            offset: 8,
            bytes: MemcmpEncodedBytes::Bytes(vec![
                marketplace::MuAccountType::ProviderRegion as u8,
            ]),
            encoding: Some(MemcmpEncoding::Binary),
        }),
        RpcFilterType::Memcmp(Memcmp {
            offset: 8 + 1,
            bytes: MemcmpEncodedBytes::Bytes(cmd.provider.to_bytes().to_vec()),
            encoding: Some(MemcmpEncoding::Binary),
        }),
    ];

    let mut accounts = client
        .program
        .accounts::<marketplace::ProviderRegion>(filters)?;

    accounts.sort_by_key(|x| x.1.region_num);

    for account in accounts {
        println!("{}. {}:", account.1.region_num, account.1.name);
        println!("\tKey: {}", account.0);
        println!("\tRates:");
        println!(
            "\t\tCPU/Memory: {}",
            account.1.rates.billion_function_mb_instructions
        );
        println!("\t\tDB Gigabytes: {}", account.1.rates.db_gigabyte_months);
        println!("\t\tDB Reads: {}", account.1.rates.million_db_reads);
        println!("\t\tDB Writes: {}", account.1.rates.million_db_writes);
        println!(
            "\t\tGateway requests: {}",
            account.1.rates.million_gateway_requests
        );
        println!(
            "\t\tBandwidth: {}",
            account.1.rates.gigabytes_gateway_traffic
        );
    }

    Ok(())
}
