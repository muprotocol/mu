use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use mu_common::pwr::VM_ID;
use pwr_rs::PrivateKey;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, read_to_string},
    path::Path,
};

use crate::pwr_client::PWRClient;

#[derive(Default, Debug, Parser)]
pub struct ConfigOverride {
    /// VM ID override.
    #[clap(global = true, long = "vm-id")]
    pub vm_id: Option<u64>,

    /// User private key file override. This wallet will be the owner of any stacks deployed
    /// during execution of commands (stacks, etc.)
    #[clap(global = true, long = "keypair", short = 'k')]
    pub private_key: Option<String>,
}

pub struct Config {
    pub vm_id: u64,
    pub private_key: Option<String>,

    signer: Option<PrivateKey>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct SerializedConfig {
    vm_id: String,
    private_key: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vm_id: VM_ID,
            private_key: None,
            signer: None,
        }
    }
}

impl Config {
    pub fn build_pwr_client(&self) -> Result<PWRClient> {
        PWRClient::new(self)
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

        if let Some(vm_id) = cfg_override.vm_id {
            cfg.vm_id = vm_id;
        }
        if let Some(private_key) = cfg_override.private_key {
            cfg.private_key = Some(private_key);
        }

        Ok(cfg)
    }

    fn from_path(p: impl AsRef<Path>) -> Result<Self> {
        let file_content = fs::read_to_string(&p).with_context(|| {
            format!("Error reading the file with path: {}", p.as_ref().display())
        })?;
        Self::deserialize_from_yaml(&file_content)
    }

    fn deserialize_from_yaml(yaml: &str) -> Result<Self> {
        let cfg: SerializedConfig = serde_yaml::from_str(yaml)
            .map_err(|e| anyhow::format_err!("Unable to deserialize config: {}", e.to_string()))?;
        Ok(Config {
            vm_id: cfg.program_id,
            private_key: Some(shellexpand::tilde(&cfg.private_key).into_owned()),
            signer: None,
        })
    }

    pub fn get_signer(&self) -> Result<PrivateKey> {
        match self.private_key {
            Some(x) => Ok(x.clone()),
            None => {
                let signer = Self::read_private_key_from_url(self.private_key.as_ref())?;

                self.signer = Some(signer.clone());
                Ok(signer)
            }
        }
    }

    pub fn read_private_key_from_url(url: Option<&String>) -> Result<PrivateKey> {
        let default_keypair_path = shellexpand::tilde("~/.config/pwr/id.json");
        match fs::metadata(&*default_keypair_path) {
                Ok(x) if x.is_file() => {
                    let content = read_to_string(default_keypair_path)?;
                    let private_key = PrivateKey::try_from(content)
                        .map_err(|f| anyhow!("Unable to read keypair file: {f}"))?;
                    Ok(private_key)
                }
                _ => bail!("No private_key specified on command line and default private_key does not exist at ~/.config/pwr/id.json, Generate a private_key and store it in one of these locations"),
            }
    }
}
