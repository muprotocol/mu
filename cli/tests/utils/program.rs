use std::process::Stdio;

use anchor_client::{
    solana_sdk::{
        self, native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, signer::Signer, system_instruction,
        system_program,
    },
    Client, Program,
};
use anyhow::{bail, Context, Result};

use crate::utils::create_rpc_client;

use super::{
    airdrop_account, common::KeypairWithPath, create_anchor_client, create_mint,
    start_test_validator, DropableChild,
};

pub fn deploy_program(owner_keypair: &KeypairWithPath) -> Result<KeypairWithPath> {
    let mut marketpalce_project_dir = std::env::current_dir()?.parent().unwrap().to_path_buf();
    marketpalce_project_dir.push("marketplace/");

    let exit = std::process::Command::new("anchor")
        .current_dir(&marketpalce_project_dir)
        .arg("build")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Must build program");
    if !exit.status.success() {
        bail!("There was a problem building program: {:?}.", exit)
    }

    let mut program_binary_path = marketpalce_project_dir.clone();
    program_binary_path.push("target/deploy/marketplace.so");

    let mut program_deploy_keypair_path = marketpalce_project_dir.clone();
    program_deploy_keypair_path.push("target/deploy/marketplace-keypair.json");

    let deploy_keypair = KeypairWithPath::load_with_path(program_deploy_keypair_path)?;

    // Send deploy transactions.
    let exit = std::process::Command::new("solana")
        .arg("program")
        .arg("deploy")
        .arg("--url")
        .arg("http://127.0.0.1:8899")
        .arg("--keypair")
        .arg(&owner_keypair.path)
        .arg("--program-id")
        .arg(&deploy_keypair.path)
        .arg(program_binary_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Must deploy");
    if !exit.status.success() {
        bail!("There was a problem deploying: {:?}.", exit)
    }
    Ok(deploy_keypair)
}

pub struct MuProgram<S>
where
    S: MuProgramState,
{
    client: Client,
    owner: KeypairWithPath,
    state: S,
}

pub trait MuProgramState {}

#[allow(dead_code)] //TODO: Remove this
pub struct Initialized {
    mint: KeypairWithPath,
    program_keypair: KeypairWithPath,
    program: Program,
    state_pda: Pubkey,
    deposit_pda: Pubkey,
}

impl MuProgramState for Initialized {}

struct Deployed {
    program_keypair: KeypairWithPath,
    program: Program,
}

impl MuProgramState for Deployed {}

impl MuProgram<Deployed> {
    pub fn deploy(client: Client, owner: KeypairWithPath) -> Result<Self> {
        let program_keypair = deploy_program(&owner)?;
        let program = client.program(program_keypair.keypair.pubkey());

        Ok(Self {
            owner,
            state: Deployed {
                program_keypair,
                program,
            },
            client,
        })
    }

    pub fn initialize(self) -> Result<MuProgram<Initialized>> {
        let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &self.state.program.id());
        let (deposit_pda, _) =
            Pubkey::find_program_address(&[b"deposit"], &self.state.program.id());

        let mint = create_mint(&self.client, &self.owner.keypair)?;

        self.state
            .program
            .request()
            .accounts(marketplace::accounts::Initialize {
                state: state_pda,
                mint: mint.keypair.pubkey(),
                deposit_token: deposit_pda,
                authority: self.owner.keypair.pubkey(),
                system_program: system_program::id(),
                token_program: spl_token::id(),
                rent: solana_sdk::sysvar::rent::id(),
            })
            .args(marketplace::instruction::Initialize)
            .signer(self.owner.keypair.as_ref())
            .send()
            .context("can not initialize program")?;

        Ok(MuProgram {
            owner: self.owner,
            client: self.client,
            state: Initialized {
                mint,
                program_keypair: self.state.program_keypair,
                program: self.state.program,
                state_pda,
                deposit_pda,
            },
        })
    }
}

impl MuProgram<Initialized> {
    pub fn create_wallet_and_associated_token_account(self) -> Result<(KeypairWithPath, Pubkey)> {
        let wallet = KeypairWithPath::new()?;

        let sig = self
            .client
            .program(system_program::id())
            .request()
            .instruction(system_instruction::transfer(
                &self.owner.keypair.pubkey(),
                &wallet.keypair.pubkey(),
                5 * LAMPORTS_PER_SOL,
            ))
            .send()
            .context("can not fund wallet")?;

        create_rpc_client().confirm_transaction(&sig).unwrap();

        println!(
            "[TEST] Balance: {:?}",
            create_rpc_client()
                .get_balance(&wallet.keypair.pubkey())
                .unwrap()
        );

        self.client
            .program(spl_associated_token_account::id())
            .request()
            .instruction(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &wallet.keypair.pubkey(),
                    &wallet.keypair.pubkey(),
                    &self.state.mint.keypair.pubkey(),
                ),
            )
            .signer(wallet.keypair.as_ref())
            .send()
            .context("can not create associated token account")
            .unwrap();

        let token_account = spl_associated_token_account::get_associated_token_address(
            &wallet.keypair.pubkey(),
            &self.state.mint.keypair.pubkey(),
        );

        println!(
            "[in create wallet and associated_token_account] Balance: {:?}",
            create_rpc_client()
                .get_balance(&wallet.keypair.pubkey())
                .unwrap()
        );

        self.client
            .program(spl_token::id())
            .request()
            .instruction(spl_token::instruction::mint_to(
                &spl_token::id(),
                &self.state.mint.keypair.pubkey(),
                &token_account,
                &wallet.keypair.pubkey(),
                &[],
                10000,
            )?)
            .signer(wallet.keypair.as_ref())
            .send()
            .context("can not mint to associated token account")?;

        println!(
            "[in create wallet and associated_token_account] Balance: {:?}",
            create_rpc_client().get_balance(&token_account).unwrap()
        );

        Ok((wallet, token_account))
    }
}

pub fn setup_env() -> Result<(MuProgram<Initialized>, DropableChild)> {
    let owner = KeypairWithPath::load_or_create_with_name("owner")?;
    let client = create_anchor_client(owner.keypair.clone())?;
    let validator_handle = start_test_validator()?;

    airdrop_account(&owner.path, 50)?;

    let mu_program = MuProgram::deploy(client, owner)?.initialize()?;

    Ok((mu_program, validator_handle))
}
