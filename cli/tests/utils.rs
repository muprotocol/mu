use std::{io::Write, path::PathBuf, process::Stdio, rc::Rc};

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
    pub fn load<S>(name: S) -> Result<Self>
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

    #[allow(dead_code)] // TODO
    pub fn new() -> Result<Self> {
        Self::_new::<&str>(None)
    }

    #[allow(dead_code)] // TODO
    pub fn load_or_create<S>(name: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let path = Self::get_keypair_path(name.as_ref())?;
        if path.try_exists()? {
            Self::load(name)
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
            .unwrap_or_else(|| keypair.pubkey().to_string());

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
        path.push("../marketplace/scripts/test-wallets/");

        if !path.is_dir() {
            std::fs::create_dir_all(&path)?;
        }

        path.push(format!("{}.json", name.as_ref()));
        Ok(path)
    }
}

pub fn create_wallet_and_associated_token_account() -> Result<(KeypairWithPath, Pubkey)> {
    let user_index = 12; //TODO: this should come from a rand run

    let exit = std::process::Command::new("anchor")
        .arg("run")
        .arg("create-and-fund-wallet")
        .arg("--")
        .arg(user_index.to_string())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir("../marketplace/")
        .output()
        .expect("Must create and fund wallet");

    if !exit.status.success() {
        bail!(
            "There was a problem creating and funding wallet: {:?}.",
            exit
        )
    }

    let mint = KeypairWithPath::load("mint")?;
    let wallet = KeypairWithPath::load(format!("user_{user_index}"))?;

    let token_account = spl_associated_token_account::get_associated_token_address(
        &wallet.pubkey(),
        &mint.pubkey(),
    );
    Ok((wallet, token_account))
}
