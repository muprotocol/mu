use std::{fs, path::PathBuf};

use anchor_client::{
    solana_client::rpc_filter::{Memcmp, RpcFilterType},
    solana_sdk::pubkey::Pubkey,
};
use anyhow::{Context, Result};
use clap::{Args, Parser};
use marketplace::StackState;

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    List(ListStacksCommand),
    Deploy(DeployStackCommand),
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
pub struct DeployStackCommand {
    #[arg(long, short('f'))]
    /// Path to the yaml file containing the stack definition.
    yaml_file: PathBuf,

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
        Command::Deploy(sub_command) => execute_deploy(config, sub_command),
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

    if stacks.is_empty() {
        println!("No stacks found");
    } else {
        for (key, stack) in stacks {
            if let StackState::Active { revision, name, .. } = stack.state {
                println!("{}:", name);
                println!("\tKey: {}", key);
                println!("\tRegion: {}", stack.region); // TODO: print region name
                println!("\tSeed: {}", stack.seed);
                println!("\tRevision: {}", revision);
            } else {
                println!("Internal error: didn't expect to receive deleted stack")
            }
        }
    }

    Ok(())
}

pub fn execute_deploy(config: Config, cmd: DeployStackCommand) -> Result<()> {
    let yaml = fs::read_to_string(cmd.yaml_file).context("Failed to read stack file")?;

    let stack = serde_yaml::from_str::<mu_stack::Stack>(yaml.as_str())
        .context("Failed to deserialize stack from YAML file")?;

    let client = config.build_marketplace_client()?;
    let user_wallet = config.get_signer()?;

    let deploy_mode = marketplace_client::stack::get_deploy_mode(cmd.init, cmd.update)?;

    marketplace_client::stack::deploy(
        &client,
        user_wallet,
        &cmd.region,
        stack,
        cmd.seed,
        deploy_mode,
    )
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

    marketplace_client::stack::delete(&client, user_wallet, &cmd.stack, region.as_ref())
}
