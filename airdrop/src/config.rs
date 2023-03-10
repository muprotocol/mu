mod serde_support;

use std::{path::PathBuf, str::FromStr};

use anyhow::{anyhow, Context, Result};
use config::{Config, Environment, File, FileFormat};
use serde::Deserialize;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{read_keypair, Keypair},
};

#[derive(Clone, Deserialize)]
pub struct AppConfig {
    pub rpc_address: String,
    pub listen_address: std::net::SocketAddr,
    pub mint_pubkey: Pubkey,
    authority_keypair: PathBuf,
    pub per_request_cap: Option<u64>,
    pub per_address_cap: Option<u64>,
    pub per_account_cap: Option<u64>,
    //pub time_slice: ConfigDuration,
}

//TODO: check that caps are valid

pub fn initialize_config() -> Result<AppConfig> {
    let defaults = vec![
        ("rpc_address", "127.0.0.1:8899"),
        ("listen_address", "127.0.0.1:0"), // 0 => Request random port from OS
    ];

    let env = Environment::default()
        .prefix("AIRDROP")
        .prefix_separator("__")
        .keep_prefix(false)
        .separator("__")
        .try_parsing(true);

    let mut builder = Config::builder();

    for (key, val) in defaults {
        builder = builder
            .set_default(key, val)
            .context("Failed to add default config")?;
    }

    builder = builder.add_source(File::new("conf.yaml", FileFormat::Yaml));

    #[cfg(debug_assertions)]
    {
        if std::path::Path::new("conf.dev.yaml").exists() {
            builder = builder.add_source(File::new("conf.dev.yaml", FileFormat::Yaml));
        }
    }

    builder = builder.add_source(env);
    let config = builder
        .build()
        .context("Failed to initialize configuration")?;

    Ok(AppConfig {
        rpc_address: config.get("rpc_address")?,
        listen_address: config.get("listen_address")?,
        mint_pubkey: config
            .get::<String>("mint_pubkey")
            .map(|p| Pubkey::from_str(&p))??,
        authority_keypair: config.get("authority_keypair")?,
        per_request_cap: config.get("per_request_cap")?,
        per_address_cap: config.get("per_address_cap")?,
        per_account_cap: config.get("per_account_cap")?,
    })
}

//TODO: Find better place for this.
impl AppConfig {
    //TODO: Support all types of Signers
    pub fn authority_keypair(&self) -> anyhow::Result<Keypair> {
        let mut file = std::fs::File::open(&self.authority_keypair)?;
        read_keypair(&mut file).map_err(|e| anyhow!("Unable to read keypair file: {e}"))
    }
}
