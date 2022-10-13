use std::{
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use anchor_client::{
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::{CommitmentConfig, CommitmentLevel},
        native_token::LAMPORTS_PER_SOL,
        pubkey::Pubkey,
        signature::{read_keypair_file, Keypair, Signature},
        signer::Signer,
        signers::Signers,
        transaction::Transaction,
    },
};
use anyhow::{anyhow, Context, Result};

pub struct KeypairWithPath {
    pub keypair: Rc<Keypair>,
    pub path: PathBuf,
}

impl KeypairWithPath {
    pub fn load_with_path<P>(path: P, binary: bool) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let keypair = if binary {
            let bytes = std::fs::read(&path)?;
            Keypair::from_bytes(&bytes).map_err(Into::into)
        } else {
            read_keypair_file(&path).map_err(|_| {
                anyhow!(
                    "Error reading keypair from file {}",
                    path.as_ref().display()
                )
            })
        }?;

        Ok(Self {
            keypair: Rc::new(keypair),
            path: path.as_ref().to_owned(),
        })
    }

    pub fn load_with_name<S>(name: S, binary: bool) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let path = Self::get_keypair_path(name)?;
        let keypair = if binary {
            let bytes = std::fs::read(&path)?;
            Keypair::from_bytes(&bytes).map_err(Into::into)
        } else {
            read_keypair_file(&path)
                .map_err(|_| anyhow!("Error reading keypair from file {}", &path.display()))
        }?;

        Ok(Self {
            keypair: Rc::new(keypair),
            path,
        })
    }

    pub fn new(store_binary: bool) -> Result<Self> {
        Self::_new::<&str>(None, store_binary)
    }

    pub fn load_or_create_with_name<S>(name: S, binary: bool) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let path = Self::get_keypair_path(name.as_ref())?;
        if path.try_exists()? {
            Self::load_with_name(name, binary)
        } else {
            Self::_new(Some(name), binary)
        }
    }

    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    fn _new<S>(name: Option<S>, store_binary: bool) -> Result<Self>
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
        if store_binary {
            file.write_all(&keypair.to_bytes())?;
        } else {
            file.write_all(format!("{:?}", &keypair.to_bytes()).as_bytes())?;
        }

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

        path.push(name.as_ref());
        Ok(path)
    }
}

pub fn create_rpc_client() -> RpcClient {
    RpcClient::new_with_commitment(
        "http://localhost:8899",
        CommitmentConfig {
            commitment: CommitmentLevel::Finalized,
        },
    )
}

pub fn send_trx<S>(rpc_client: &RpcClient, mut trx: Transaction, signers: &S) -> Result<Signature>
where
    S: Signers,
{
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    trx.sign(signers, recent_blockhash);

    rpc_client
        .send_and_confirm_transaction_with_spinner(&trx)
        .map_err(Into::into)
}

pub fn request_airdrop(rpc_client: &RpcClient, to_pubkey: &Pubkey, lamports: u64) -> Result<()> {
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let signature =
        rpc_client.request_airdrop_with_blockhash(to_pubkey, lamports, &recent_blockhash)?;
    rpc_client.confirm_transaction_with_spinner(
        &signature,
        &recent_blockhash,
        CommitmentConfig {
            commitment: CommitmentLevel::Finalized,
        },
    )?;
    Ok(())
}

pub fn create_wallet_and_associated_token_account(
    rpc_client: &RpcClient,
) -> Result<KeypairWithPath> {
    let wallet = KeypairWithPath::new(false)?;
    let mint = KeypairWithPath::load_with_name("mint", true)?;

    request_airdrop(rpc_client, &wallet.pubkey(), 1 * LAMPORTS_PER_SOL)?;

    let ins = spl_associated_token_account::instruction::create_associated_token_account(
        &wallet.pubkey(),
        &wallet.pubkey(),
        &mint.pubkey(),
    );
    let trx = Transaction::new_with_payer(&[ins], Some(&wallet.pubkey()));

    send_trx(rpc_client, trx, &[wallet.keypair.as_ref()])?;
    Ok(wallet)
}
