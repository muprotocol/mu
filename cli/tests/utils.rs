use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    rc::Rc,
};

use anchor_client::solana_sdk::{
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair},
    signer::Signer,
};
use anyhow::{anyhow, bail, Context, Result};

pub struct KeypairWithPath {
    pub keypair: Rc<Keypair>,
    pub path: PathBuf,
}

impl KeypairWithPath {
    pub fn load_with_path<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let keypair = read_keypair_file(path.as_ref())
            .map_err(|_| anyhow!("Error reading keypair from file"))?;

        Ok(Self {
            keypair: Rc::new(keypair),
            path: path.as_ref().to_owned(),
        })
    }

    pub fn load_with_name<S>(name: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let path = Self::get_keypair_path(name)?;
        let keypair = read_keypair_file(&path)
            .map_err(|_| anyhow!("Error reading keypair from file {}", &path.display()))?;

        Ok(Self {
            keypair: Rc::new(keypair),
            path,
        })
    }

    pub fn new() -> Result<Self> {
        Self::_new::<&str>(None)
    }

    pub fn load_or_create_with_name<S>(name: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let path = Self::get_keypair_path(name.as_ref())?;
        if path.try_exists()? {
            Self::load_with_name(name)
        } else {
            Self::_new(Some(name))
        }
    }

    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    fn _new<S>(name: Option<S>) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let keypair = Keypair::new();

        let name = name
            .map(|i| i.as_ref().to_string())
            .unwrap_or(keypair.pubkey().to_string());

        let path = Self::get_keypair_path(name)?;

        let mut file = std::fs::File::create(&path).context(format!(
            "Error creating file with path: {}",
            path.to_string_lossy()
        ))?;
        file.write_all(format!("{:?}", &keypair.to_bytes()).as_bytes())?;

        Ok(Self {
            keypair: Rc::new(keypair),
            path,
        })
    }

    fn get_keypair_path<S>(name: S) -> Result<PathBuf>
    where
        S: AsRef<str>,
    {
        let mut path = std::env::current_dir()?;
        path.push("target/keypairs/");

        if !path.is_dir() {
            std::fs::create_dir_all(&path)?;
        }

        path.push(format!("{}.json", name.as_ref()));
        Ok(path)
    }
}

pub fn airdrop_account(keypair_path: &Path, sols: u64) -> Result<()> {
    let exit = std::process::Command::new("solana")
        .arg("airdrop")
        .arg("--url")
        .arg("http://127.0.0.1:8899")
        .arg(sols.to_string())
        .arg("--keypair")
        .arg(keypair_path.display().to_string())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Must get airdrop");

    if !exit.status.success() {
        bail!("There was a problem getting airdrop: {:?}.", exit)
    }
    Ok(())
}
