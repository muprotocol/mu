use std::path::PathBuf;

use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat};
use serde::Deserialize;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};

#[derive(Deserialize)]
pub struct AppConfig {
    listen_address: std::net::SocketAddr,
    mint_pubkey: Pubkey,
    authority_keypair: PathBuf,
    per_address_cap: u64,
    per_hour_cap: u64,
}

pub fn initialize_config() -> Result<AppConfig> {
    let defaults = vec![
        ("listen_address", "127.0.0.1:0"), // 0 => Random port from OS
        ("per_address_cap", "10000"),
        ("per_hour_cap", "1000"),
    ];

    let env = Environment::default()
        .prefix("MU")
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

    builder = builder.add_source(File::new("mu-conf.yaml", FileFormat::Yaml));

    #[cfg(debug_assertions)]
    {
        if std::path::Path::new("mu-conf.dev.yaml").exists() {
            builder = builder.add_source(File::new("mu-conf.dev.yaml", FileFormat::Yaml));
        }
    }

    builder = builder.add_source(env);

    builder
        .build()
        .context("Failed to initialize configuration")?
        .try_deserialize()
        .map_err(Into::into)
}

impl AppConfig {
    // Support all types of Signers
    pub fn authority_keypair(&self) -> anyhow::Result<Keypair> {
        let bytes = std::fs::read(&self.authority_keypair)?;
        Keypair::from_bytes(&bytes).map_err(Into::into)
    }
}
