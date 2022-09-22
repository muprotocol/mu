use std::{error::Error, io::Write, process::Stdio, rc::Rc};

use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{read_keypair_file, Keypair},
        signer::Signer,
        system_instruction, system_program,
    },
    Client, Cluster,
};
use anchor_spl::token::Mint;
use anyhow::{bail, Context};

fn create_client(cluster: Cluster, provider: String) -> Result<Client, Box<dyn Error>> {
    let payer = read_keypair_file(provider)?;
    Ok(Client::new_with_options(
        cluster,
        Rc::new(payer),
        CommitmentConfig::processed(),
    ))
}

fn create_mint(client: &Client, payer: &Keypair) -> Result<Keypair, Box<dyn Error>> {
    let mint = Keypair::new();

    let program = client.program(system_program::ID);

    program
        .request()
        .instruction(system_instruction::create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            program
                .rpc()
                .get_minimum_balance_for_rent_exemption(Mint::LEN)?,
            Mint::LEN.try_into()?,
            &mint.pubkey(),
        ))
        .instruction(spl_token::instruction::initialize_mint(
            &spl_token::ID,
            &payer.pubkey(),
            &payer.pubkey(),
            None,
            6,
        )?)
        .send()?;

    Ok(mint)
}

fn create_and_fund_wallet(
    client: &Client,
    payer: Box<dyn Signer>,
    mint: Keypair,
) -> Result<(Keypair, Pubkey), Box<dyn Error>> {
    let wallet = Keypair::new();

    client
        .program(system_program::ID)
        .request()
        .instruction(system_instruction::transfer(
            &payer.pubkey(),
            &wallet.pubkey(),
            5 * 10u64.pow(spl_token::native_mint::DECIMALS.into()),
        ))
        .send()?;

    let associated_token_program = client.program(spl_associated_token_account::ID);

    associated_token_program
        .request()
        .instruction(
            spl_associated_token_account::instruction::create_associated_token_account(
                &wallet.pubkey(),
                &wallet.pubkey(),
                &mint.pubkey(),
            ),
        )
        .signer(&wallet)
        .send()?;

    let token_account = spl_associated_token_account::get_associated_token_address(
        &wallet.pubkey(),
        &mint.pubkey(),
    );

    associated_token_program
        .request()
        .instruction(spl_token::instruction::mint_to(
            &spl_token::ID,
            &mint.pubkey(),
            &token_account,
            &payer.pubkey(),
            &[&mint.pubkey()],
            10000,
        )?)
        .send()?;

    Ok((wallet, token_account))
}

fn deploy_marketplace(cluster_url: String, owner_keypair_path: String) -> anyhow::Result<()> {
    let mut program_keypair_path = std::env::current_dir()?;
    program_keypair_path.push("target/marketplace_deployed_keypair.json");

    let mut program_binary_path = std::env::current_dir()?.parent().unwrap().to_path_buf();
    program_binary_path.push("marketplace/programs/marketplace/target/deploy/lib.sol");

    let program_keypair = Keypair::new();
    let mut file = std::fs::File::create(&program_keypair_path).context(format!(
        "Error creating file with path: {}",
        program_keypair_path.to_string_lossy()
    ))?;
    file.write_all(format!("{:?}", &program_keypair.to_bytes()).as_bytes())?;

    // Send deploy transactions.
    let exit = std::process::Command::new("solana")
        .arg("program")
        .arg("deploy")
        .arg("--url")
        .arg(&cluster_url)
        .arg("--keypair")
        .arg(&owner_keypair_path)
        .arg("--program-id")
        .arg(program_keypair_path)
        .arg(program_binary_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Must deploy");
    if !exit.status.success() {
        bail!("There was a problem deploying: {:?}.", exit)
    }
    Ok(())
}
