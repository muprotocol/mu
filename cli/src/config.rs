//! TODO: more descriptive error context messages

use anchor_client::{
    solana_sdk::{pubkey::Pubkey, signature::read_keypair_file, signer::Signer},
    Cluster,
};
use anyhow::{anyhow, bail, Context, Error, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use solana_cli_config::{Config as SolanaConfig, CONFIG_FILE};
use solana_remote_wallet::remote_wallet::RemoteWalletManager;
use std::{cell::RefCell, fs, io, path::Path, rc::Rc, str::FromStr, sync::Arc};

use crate::{marketplace_client::MarketplaceClient, signer};

#[derive(Default, Debug, Parser)]
pub struct ConfigOverride {
    // TODO: why override the program ID? At best, we'd only want to do this during development,
    // so it should be behind a feature/debug_assertions flag.
    /// Program ID override.
    #[clap(global = true, long = "program-id")]
    pub program_id: Option<Pubkey>,

    /// Cluster override.
    #[clap(global = true, long = "cluster", short = 'u')]
    pub cluster: Option<Cluster>,

    /// User keypair override. This wallet will be the owner of any accounts created
    /// during execution of commands (providers, stacks, etc.)
    #[clap(global = true, long = "keypair", short = 'k')]
    pub keypair: Option<String>,

    #[clap(global = true, long = "skip-seed-phrase-validation")]
    pub skip_seed_phrase_validation: bool,

    #[clap(global = true, long = "confirm-key")]
    pub confirm_key: bool,
}

pub struct Config {
    // TODO: see TODOs in `ConfigOverride`
    pub program_id: Pubkey,
    pub cluster: Cluster,
    pub keypair: Option<String>,

    skip_seed_phrase_validation: bool,
    confirm_key: bool,

    signer: RefCell<Option<Rc<dyn Signer>>>,
    wallet_manager: RefCell<Option<Arc<RemoteWalletManager>>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            program_id: marketplace::id(),
            cluster: {
                #[cfg(debug_assertions)]
                {
                    Cluster::Localnet
                }

                #[cfg(not(debug_assertions))]
                {
                    Cluster::Mainnet
                }
            },
            keypair: None,
            skip_seed_phrase_validation: false,
            confirm_key: false,
            signer: RefCell::new(None),
            wallet_manager: RefCell::new(None),
        }
    }
}

impl Config {
    pub fn build_marketplace_client(&self) -> Result<MarketplaceClient> {
        MarketplaceClient::new(self)
    }

    pub fn discover(cfg_override: ConfigOverride) -> Result<Config> {
        let mut cfg = {
            let path = std::env::current_exe()?.with_file_name("mu-cli.yaml");

            if path.try_exists()? {
                let cfg = Self::from_path(&path)?;
                anyhow::Ok(Some(cfg))
            } else {
                // Check in the current directory when running in debug mode, this
                // helps discover the config from the project directory when running
                // with `cargo run`
                #[cfg(debug_assertions)]
                {
                    let mut path = std::env::current_dir()?;
                    path.push("mu-cli.yaml");

                    if path.try_exists()? {
                        let cfg = Self::from_path(&path)?;
                        anyhow::Ok(Some(cfg))
                    } else {
                        anyhow::Ok(None)
                    }
                }

                #[cfg(not(debug_assertions))]
                {
                    anyhow::Ok(None)
                }
            }
        }?
        .unwrap_or_default();

        if let Some(program_id) = cfg_override.program_id {
            cfg.program_id = program_id;
        }
        if let Some(cluster) = cfg_override.cluster {
            cfg.cluster = cluster;
        }
        if let Some(keypair) = cfg_override.keypair {
            cfg.keypair = Some(keypair);
        }

        cfg.skip_seed_phrase_validation = cfg_override.skip_seed_phrase_validation;
        cfg.confirm_key = cfg_override.confirm_key;

        Ok(cfg)
    }

    fn from_path(p: impl AsRef<Path>) -> Result<Self> {
        fs::read_to_string(&p)
            .with_context(|| format!("Error reading the file with path: {}", p.as_ref().display()))?
            .parse::<Self>()
    }

    pub fn get_signer(&self) -> Result<Rc<dyn Signer>> {
        let signer_ref = self.signer.borrow();
        match signer_ref.as_ref() {
            Some(x) => Ok(x.clone()),
            None => {
                // Drop the ref so we can re-borrow down below
                drop(signer_ref);

                let (signer, wallet_manager) = Self::read_keypair_from_url(
                    self.keypair.as_ref(),
                    self.skip_seed_phrase_validation,
                    self.confirm_key,
                )?;

                *self.signer.borrow_mut() = Some(signer.clone());
                *self.wallet_manager.borrow_mut() = wallet_manager;
                Ok(signer)
            }
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn read_keypair_from_url(
        url: Option<&String>,
        skip_seed_phrase_validation: bool,
        confirm_key: bool,
    ) -> Result<(Rc<dyn Signer>, Option<Arc<RemoteWalletManager>>)> {
        fn read_default_keypair_file() -> Result<Rc<dyn Signer>> {
            let default_keypair_path = shellexpand::tilde("~/.config/solana/id.json");
            match fs::metadata(&*default_keypair_path) {
                Ok(x) if x.is_file() => {
                    let keypair = read_keypair_file(&*default_keypair_path)
                        .map_err(|f| anyhow!("Unable to read keypair file: {f}"))?;
                    let rc: Rc<dyn Signer> = Rc::new(keypair);
                    Ok(rc)
                }
                _ => bail!("No keypair specified on command line and default keypair does not exist at ~/.config/solana/id.json, use `solana-keygen new` to generate a keypair"),
            }
        }

        match &url {
            None => read_default_keypair_file().map(|x| (x, None)),
            Some(keypair) => {
                let mut wallet_manager = None;
                let config = signer::SignerFromPathConfig {
                    skip_seed_phrase_validation,
                    confirm_key,
                };
                signer::signer_from_path(keypair.as_str(), "keypair", &mut wallet_manager, &config)
                    .context("Failed to read keypair")
                    .map(|b| (b.into(), wallet_manager))
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
// TODO: naming, also why have two of this?
struct _Config {
    program_id: String,
    cluster: String,
    keypair: String,
}

// TODO: see ToString above
impl FromStr for Config {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let cfg: _Config = serde_yaml::from_str(s)
            .map_err(|e| anyhow::format_err!("Unable to deserialize config: {}", e.to_string()))?;
        Ok(Config {
            cluster: cfg.cluster.parse()?,
            keypair: Some(shellexpand::tilde(&cfg.keypair).into_owned()),
            program_id: Pubkey::from_str(&cfg.program_id)?,
            skip_seed_phrase_validation: false,
            confirm_key: false,
            signer: RefCell::new(None),
            wallet_manager: RefCell::new(None),
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
