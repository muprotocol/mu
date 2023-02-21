use std::io::{stdin, Read};

use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine};
use clap::Parser;

use crate::{config::Config, marketplace_client};

#[derive(Debug, Parser)]
pub enum Command {
    Create(SignerSpecificationCommand),
    Activate(SignerSpecificationCommand),
    Deactivate(SignerSpecificationCommand),
    SignRequest(SignRequestCommand),
}

#[derive(Debug, Parser)]
pub struct SignerSpecificationCommand {
    #[arg(long)]
    pub signer_keypair: String,

    #[arg(long)]
    pub signer_skip_seed_phrase_validation: bool,

    #[arg(long)]
    pub signer_confirm_key: bool,

    #[arg(long)]
    pub region: Pubkey,
}

#[derive(Debug, Parser)]
pub struct SignRequestCommand {
    #[arg(long)]
    /// The keypair to sign with. If not specified, the user's wallet will be used
    pub signer_keypair: Option<String>,

    #[arg(long)]
    pub signer_skip_seed_phrase_validation: bool,

    #[arg(long)]
    pub signer_confirm_key: bool,

    payload: String,
}

pub fn execute(config: Config, command: Command) -> Result<()> {
    match command {
        Command::Create(args) => execute_create(config, args),
        Command::Activate(args) => execute_activate(config, args),
        Command::Deactivate(args) => execute_deactivate(config, args),
        Command::SignRequest(args) => execute_sign_request(args),
    }
}

fn execute_create(config: Config, args: SignerSpecificationCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let user_wallet = config.get_signer()?;

    let (signer, wallet_manager) = Config::read_keypair_from_url(
        Some(&args.signer_keypair),
        args.signer_skip_seed_phrase_validation,
        args.signer_confirm_key,
    )?;

    marketplace_client::request_signer::create(
        &client,
        user_wallet.as_ref(),
        signer.as_ref(),
        &args.region,
    )?;

    drop(wallet_manager);
    Ok(())
}

fn execute_activate(config: Config, args: SignerSpecificationCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let user_wallet = config.get_signer()?;

    let (signer, wallet_manager) = Config::read_keypair_from_url(
        Some(&args.signer_keypair),
        args.signer_skip_seed_phrase_validation,
        args.signer_confirm_key,
    )?;

    marketplace_client::request_signer::activate(
        &client,
        user_wallet.as_ref(),
        signer.as_ref(),
        &args.region,
    )?;

    drop(wallet_manager);
    Ok(())
}

fn execute_deactivate(config: Config, args: SignerSpecificationCommand) -> Result<()> {
    let client = config.build_marketplace_client()?;
    let user_wallet = config.get_signer()?;

    let (signer, wallet_manager) = Config::read_keypair_from_url(
        Some(&args.signer_keypair),
        args.signer_skip_seed_phrase_validation,
        args.signer_confirm_key,
    )?;

    marketplace_client::request_signer::deactivate(
        &client,
        user_wallet.as_ref(),
        &signer.pubkey(),
        &args.region,
    )?;

    drop(wallet_manager);
    Ok(())
}

fn execute_sign_request(args: SignRequestCommand) -> Result<()> {
    let payload = if args.payload == "-" {
        let mut buf = vec![];
        stdin()
            .read_to_end(&mut buf)
            .context("Failed to read from stdin")?;
        String::from_utf8(buf).map_err(|_| anyhow!("Input is not valid unicode"))?
    } else {
        args.payload
    };

    let (signer, wallet_manager) = Config::read_keypair_from_url(
        args.signer_keypair.as_ref(),
        args.signer_skip_seed_phrase_validation,
        args.signer_confirm_key,
    )?;

    let signature = signer.try_sign_message(
        payload
            .trim_end_matches(|c: char| c.is_ascii_control())
            .as_bytes(),
    )?;
    let sig_base64 = general_purpose::STANDARD.encode(signature.as_ref());
    let pubkey = signer.pubkey();
    let pk_base64 = general_purpose::STANDARD.encode(pubkey.to_bytes());

    println!("X-MU-PUBLIC-KEY: {pk_base64}");
    println!("X-MU-SIGNATURE: {sig_base64}");

    drop(wallet_manager);
    Ok(())
}
