use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    rc::Rc,
};

use anchor_client::{
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        signature::{read_keypair_file, Keypair},
        signer::Signer,
        system_instruction, system_program,
    },
    Client, Cluster,
};
use anchor_spl::token::Mint;
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

pub fn create_anchor_client(payer: Rc<Keypair>) -> Result<Client> {
    Ok(Client::new_with_options(
        Cluster::Localnet,
        payer,
        CommitmentConfig::processed(),
    ))
}

pub fn create_rpc_client() -> RpcClient {
    RpcClient::new("http://127.0.0.1:8899")
}

pub fn create_mint(client: &Client, payer: &Keypair) -> Result<KeypairWithPath> {
    let mint = KeypairWithPath::new()?;
    let program = client.program(system_program::ID);
    let min_balance = program
        .rpc()
        .get_minimum_balance_for_rent_exemption(Mint::LEN)?;

    program
        .request()
        .instruction(system_instruction::create_account(
            &payer.pubkey(),
            &mint.keypair.pubkey(),
            min_balance,
            Mint::LEN as u64,
            &spl_token::ID,
        ))
        .instruction(spl_token::instruction::initialize_mint(
            &spl_token::ID,
            &mint.keypair.pubkey(),
            &payer.pubkey(),
            Some(&payer.pubkey()),
            6,
        )?)
        .signer(mint.keypair.as_ref())
        .send()
        .context("can not create mint")?;

    Ok(mint)
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

pub struct DropableChild(Child);

impl DropableChild {
    pub fn new(child: Child) -> Self {
        Self(child)
    }
}

impl Drop for DropableChild {
    fn drop(&mut self) {
        self.0.kill().unwrap();
    }
}

pub fn start_test_validator() -> Result<DropableChild> {
    let mut validator_handle = std::process::Command::new("solana-test-validator")
        //.arg("-q")
        .arg("--log")
        .arg("-r")
        .arg("-l")
        .arg("target/test-ledger")
        .env("RUST_LOG", "solana_runtime::system_instruction_processor=trace,solana_runtime::message_processor=debug,solana_bpf_loader=debug,solana_rbpf=debug")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::format_err!("{}", e.to_string()))?;

    // Wait for the validator to be ready.
    let client = create_rpc_client();
    let mut count = 0;
    while count < 3000 {
        // 3 seconds
        let r = client.get_latest_blockhash();
        if r.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        count += 1;
    }
    if count == 3000 {
        eprintln!("Unable to get latest blockhash. Test validator does not look started.",);
        validator_handle.kill()?;
        std::process::exit(1);
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    Ok(DropableChild::new(validator_handle))
}
