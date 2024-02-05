use std::collections::{HashMap, HashSet};

use anchor_client::{
    solana_client::rpc_filter::{Memcmp, RpcFilterType},
    solana_sdk::pubkey::Pubkey,
};
use anyhow::{Context, Result};
use clap::{Args, Parser};
use marketplace::StackState;

use crate::{config::Config, pwr_client};

#[derive(Debug, Parser)]
pub enum Command {
    List(ListStacksCommand),
    Delete(DeleteStackCommand),
}

#[derive(Debug, Args)]
pub struct ListStacksCommand {
    // TODO: implement provider-scope search by retrieving the provider's regions and
    // making requests for each region
    #[arg(long)]
    /// Limit results to stacks deployed to this region only.
    region: Option<Pubkey>,

    #[arg(long)]
    /// Perform a prefix search on stack names (case-sensitive).
    name_prefix: Option<String>,
}

#[derive(Debug, Args)]
pub struct DeleteStackCommand {
    /// The ID of the stack to be deleted.
    stack: Pubkey,

    #[arg(short, long)]
    /// The region the stack is deployed to. This is included
    /// as a safeguard against accidentally deleting the wrong
    /// stack. If you don't wish to specify the region, you can
    /// pass '--region any' to this tool.
    region: String,
}

pub fn execute(config: Config, cmd: Command) -> Result<()> {
    match cmd {
        Command::List(sub_command) => execute_list(config, sub_command),
        Command::Delete(sub_command) => execute_delete(config, sub_command),
    }
}

pub fn execute_list(config: Config, cmd: ListStacksCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let user_wallet = config.get_signer()?;

    let mut filters = vec![
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8,
            user_wallet.pubkey().to_bytes().to_vec(),
        )),
        RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8 + 32 + 32 + 8 + 1,
            vec![marketplace::StackStateDiscriminator::Active as u8],
        )),
    ];

    if let Some(region) = cmd.region {
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8 + 32,
            region.to_bytes().to_vec(),
        )));
    }

    if let Some(name_prefix) = cmd.name_prefix {
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8 + 32 + 32 + 8 + 4 + 1 + 4,
            name_prefix.as_bytes().to_vec(),
        )))
    }

    let stacks = client
        .program
        .accounts::<marketplace::Stack>(filters)
        .context("Failed to fetch stacks from blockchain")?;

    let regions = stacks
        .iter()
        .map(|s| s.1.region)
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|id| {
            client
                .program
                .account::<marketplace::ProviderRegion>(id)
                .map(|r| (id, r))
                .map_err(Into::into)
        })
        .collect::<Result<HashMap<_, _>>>()
        .context("Failed to fetch regions")?;

    let providers = regions
        .iter()
        .map(|s| s.1.provider)
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|id| {
            client
                .program
                .account::<marketplace::Provider>(id)
                .map(|r| (id, r))
                .map_err(Into::into)
        })
        .collect::<Result<HashMap<_, _>>>()
        .context("Failed to fetch providers")?;

    if stacks.is_empty() {
        println!("No stacks found");
    } else {
        for (key, stack) in stacks {
            if let StackState::Active { revision, name, .. } = stack.state {
                let region = regions.get(&stack.region).unwrap();
                let provider = providers.get(&region.provider).unwrap();
                println!("{name}:");
                println!("\tKey: {key}");
                println!("\tProvider ID: {}", region.provider);
                println!("\tProvider Name: {}", provider.name);
                println!("\tRegion ID: {}", stack.region);
                println!("\tRegion Name: {}", region.name);
                println!("\tSeed: {}", stack.seed);
                println!("\tRevision: {revision}");
            } else {
                println!("Internal error: didn't expect to receive deleted stack")
            }
        }
    }

    Ok(())
}

pub fn execute_delete(config: Config, cmd: DeleteStackCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let user_wallet = config.get_signer()?;

    let region = {
        if cmd.region == "any" {
            None
        } else {
            Some(cmd.region.parse::<Pubkey>()?)
        }
    };

    pwr_client::stack::delete(&client, user_wallet, &cmd.stack, region.as_ref())
}
