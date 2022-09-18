//! The configuration that will be used in CLI
//TODO: add logging

use anchor_client::Cluster;
use anyhow::{Context, Result};
use config::{Config, Environment, File, FileFormat};
use serde::Deserialize;

use crate::error::MuCliError;

/// Mu CLI Configurations
#[derive(Deserialize)]
pub struct MuCliConfig {
    /// The cluster to use, (Mainnet, Devnet, Testnet, Custom, ...)
    pub cluster: Cluster,

    /// Keypair of the signer to be used in operations
    pub keypair_path: Option<String>,
}

impl MuCliConfig {
    /// Initialize configurations from config files and env vars
    pub fn initialize() -> Result<MuCliConfig> {
        let solana_config_file = solana_cli_config::CONFIG_FILE
            .as_ref()
            .ok_or_else(|| MuCliError::ConfigFileNotFound)?;
        let solana_cli_config = solana_cli_config::Config::load(&solana_config_file)
            .context("Can not read solana cli config")?;

        let defaults = vec![
            ("cluster", solana_cli_config.websocket_url),
            ("keypair_path", solana_cli_config.keypair_path),
        ];

        let env = Environment::default()
            .prefix("MUCLI")
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

        builder = builder.add_source(File::new("mucli-conf.yaml", FileFormat::Yaml));

        #[cfg(debug_assertions)]
        {
            if std::path::Path::new("mucli-conf.dev.yaml").exists() {
                builder = builder.add_source(File::new("mucli-conf.dev.yaml", FileFormat::Yaml));
            }
        }

        builder = builder.add_source(env);

        builder
            .build()
            .context("Failed to initialize configuration")?
            .try_deserialize()
            .context("Invalid configuration")
    }
}
