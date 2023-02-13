use anyhow::Result;
use base64::{engine::general_purpose, Engine};
use clap::Parser;

use crate::config::Config;

#[derive(Debug, Parser, Clone)]
pub struct Command {
    payload: String,
}

pub fn execute(config: Config, command: Command) -> Result<()> {
    let signer = config.get_signer()?;
    let signature = signer.try_sign_message(command.payload.as_bytes())?;
    let sig_base64 = general_purpose::STANDARD.encode(signature.as_ref());
    let pubkey = signer.pubkey();
    let pk_base64 = general_purpose::STANDARD.encode(pubkey.to_bytes());

    println!("X-MU-PUBLIC-KEY: {}", pk_base64);
    println!("X-MU-SIGNATURE: {}", sig_base64);

    Ok(())
}
