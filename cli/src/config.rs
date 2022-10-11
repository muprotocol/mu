use anchor_client::{
    solana_sdk::{
        pubkey::Pubkey,
        signature::{read_keypair_file, Keypair},
    },
    Cluster,
};
use anyhow::{anyhow, Context, Error, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use solana_cli_config::{Config as SolanaConfig, CONFIG_FILE};
use std::{fs, io, path::Path, str::FromStr};

#[derive(Default, Debug, Parser)]
pub struct ConfigOverride {
    /// Program ID override.
    #[clap(global = true, long = "program-id")]
    pub program_id: Option<Pubkey>,
    /// Cluster override.
    #[clap(global = true, long = "cluster")]
    pub cluster: Option<Cluster>,
    /// payer override.
    #[clap(global = true, long = "wallet")]
    pub payer: Option<PayerWalletPath>,
}

#[derive(Debug)]
pub struct Config {
    pub program_id: Pubkey,
    pub cluster: Cluster,
    pub payer: PayerWalletPath,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            program_id: marketplace::id(),
            cluster: Cluster::default(),
            payer: PayerWalletPath::default(),
        }
    }
}

impl Config {
    pub fn discover(cfg_override: &ConfigOverride) -> Result<Config> {
        let mut cfg = Config::_discover()?.unwrap_or_default();

        if let Some(program_id) = cfg_override.program_id.clone() {
            cfg.program_id = program_id;
        }
        if let Some(cluster) = cfg_override.cluster.clone() {
            cfg.cluster = cluster;
        }
        if let Some(payer) = cfg_override.payer.clone() {
            cfg.payer = payer;
        }

        Ok(cfg)
    }

    fn _discover() -> Result<Option<Config>> {
        let mut path = std::env::current_dir()?;
        path.push("mu-cli.yaml");

        if path.try_exists()? {
            let cfg = Self::from_path(&path)?;
            Ok(Some(cfg))
        } else {
            Ok(None)
        }
    }

    fn from_path(p: impl AsRef<Path>) -> Result<Self> {
        fs::read_to_string(&p)
            .with_context(|| format!("Error reading the file with path: {}", p.as_ref().display()))?
            .parse::<Self>()
    }

    pub fn payer_kp(&self) -> Result<Keypair> {
        read_keypair_file(&self.payer.to_string())
            .map_err(|_| anyhow!("Unable to read keypair file"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct _Config {
    program_id: String,
    cluster: String,
    payer: String,
}

impl ToString for Config {
    fn to_string(&self) -> String {
        let cfg = _Config {
            cluster: format!("{}", self.cluster),
            payer: self.payer.to_string(),
            program_id: self.program_id.to_string(),
        };

        serde_yaml::to_string(&cfg).expect("Must be well formed")
    }
}

impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let cfg: _Config = serde_yaml::from_str(s)
            .map_err(|e| anyhow::format_err!("Unable to deserialize config: {}", e.to_string()))?;
        Ok(Config {
            cluster: cfg.cluster.parse()?,
            payer: shellexpand::tilde(&cfg.payer).parse()?,
            program_id: Pubkey::from_str(&cfg.program_id)?,
        })
    }
}

pub fn get_solana_cfg_url() -> Result<String, io::Error> {
    let config_file = CONFIG_FILE.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Default Solana config was not found",
        )
    })?;
    SolanaConfig::load(config_file).map(|config| config.json_rpc_url)
}

crate::home_path!(PayerWalletPath, ".config/solana/id.json");
