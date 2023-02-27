use std::rc::Rc;

use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer, system_program};
use anyhow::{bail, Context, Result};

use super::MarketplaceClient;

pub enum DeployMode {
    InitOnly,
    UpdateOnly,
    Automatic,
}

pub fn deploy(
    client: &MarketplaceClient,
    user_wallet: Rc<dyn Signer>,
    region_pda: &Pubkey,
    stack: mu_stack::Stack,
    seed: u64,
    deploy_mode: DeployMode,
) -> Result<()> {
    let name = stack.name.clone();

    if !client.account_exists(region_pda)? {
        bail!("There is no such region registered with this provider");
    }

    let stack_pda = client.get_stack_pda(&user_wallet.pubkey(), region_pda, seed);

    let update = {
        let existing = client
            .try_account::<marketplace::Stack>(&stack_pda)
            .context("Failed to fetch stack from Solana")?;

        match (deploy_mode, existing) {
            (
                _,
                Some(marketplace::Stack {
                    state: marketplace::StackState::Deleted,
                    ..
                }),
            ) => bail!(
                "A stack was previously deployed to this region with the same seed, and was \
                then deleted. You cannot deploy a new stack with the same seed as a deleted \
                stack. To deploy a new stack, choose a different seed."
            ),

            (DeployMode::InitOnly, Some(_)) => {
                bail!("Stack already exists, cannot initialize again")
            }
            (DeployMode::UpdateOnly, None) => bail!("Stack was not initialized, cannot update"),
            (DeployMode::InitOnly, None) | (DeployMode::Automatic, None) => false,
            (
                DeployMode::UpdateOnly,
                Some(marketplace::Stack {
                    state: marketplace::StackState::Active { stack_data, .. },
                    ..
                }),
            )
            | (
                DeployMode::Automatic,
                Some(marketplace::Stack {
                    state: marketplace::StackState::Active { stack_data, .. },
                    ..
                }),
            ) => {
                ensure_newer_version(&stack_data, &stack)?;
                true
            }
        }
    };

    let stack_version = stack.version.clone();
    let proto = stack
        .serialize_to_proto()
        .context("Failed to serialize stack to binary format")?;

    let region = client
        .program
        .account::<marketplace::ProviderRegion>(*region_pda)
        .context("Failed to fetch region from Solana")?;

    if update {
        let accounts = marketplace::accounts::UpdateStack {
            region: *region_pda,
            stack: stack_pda,
            user: user_wallet.pubkey(),
            system_program: system_program::id(),
        };

        let instruction = marketplace::instruction::UpdateStack {
            _stack_seed: seed,
            stack_data: proto.to_vec(),
            name,
        };

        client
            .program
            .request()
            .accounts(accounts)
            .args(instruction)
            .signer(user_wallet.as_ref())
            .send_with_spinner_and_config(Default::default())
            .context("Failed to send stack update transaction")?;

        println!("Stack successfully updated to version {stack_version} with key: {stack_pda}");
    } else {
        let accounts = marketplace::accounts::CreateStack {
            region: *region_pda,
            provider: region.provider,
            stack: stack_pda,
            user: user_wallet.pubkey(),
            system_program: system_program::id(),
        };

        let instruction = marketplace::instruction::CreateStack {
            stack_seed: seed,
            stack_data: proto.to_vec(),
            name,
        };

        client
            .program
            .request()
            .accounts(accounts)
            .args(instruction)
            .signer(user_wallet.as_ref())
            .send_with_spinner_and_config(Default::default())
            .context("Failed to send stack creation transaction")?;

        println!("Stack deployed successfully with version {stack_version} and key: {stack_pda}");
    }

    Ok(())
}

fn ensure_newer_version(existing: &Vec<u8>, new: &mu_stack::Stack) -> Result<()> {
    // This function's name is a bit misleading. We don't use semver, so we can't
    // really ensure the new stack has a *newer* version, just that it doesn't
    // have the *same* version as the existing one.

    let existing = mu_stack::Stack::try_deserialize_proto(existing)
        .context("Failed to deserialize existing stack")?;

    if new.version == existing.version {
        bail!(
            "This stack is already deployed with the same version ({}); \
            if you are deploying a more recent version, make sure to update \
            the stack's version in the YAML manifest.",
            existing.version
        );
    }

    Ok(())
}

pub fn get_deploy_mode(init: bool, update: bool) -> Result<DeployMode> {
    match (init, update) {
        (true, true) => bail!("Cannot specify both init and update simultaneously"),
        (true, false) => Ok(DeployMode::InitOnly),
        (false, true) => Ok(DeployMode::UpdateOnly),
        (false, false) => Ok(DeployMode::Automatic),
    }
}

pub fn delete(
    client: &MarketplaceClient,
    user_wallet: Rc<dyn Signer>,
    stack_pda: &Pubkey,
    region_pda: Option<&Pubkey>,
) -> Result<()> {
    let stack = match client.try_account::<marketplace::Stack>(stack_pda)? {
        Some(x) => x,
        None => bail!("There is no stack with this ID"),
    };

    if let Some(region) = region_pda {
        if stack.region != *region {
            bail!(
                "The stack is not in the specified region; it is in {}",
                stack.region
            );
        }
    }

    let accounts = marketplace::accounts::DeleteStack {
        region: stack.region,
        stack: *stack_pda,
        user: user_wallet.pubkey(),
        system_program: system_program::id(),
    };

    let instruction = marketplace::instruction::DeleteStack {
        _stack_seed: stack.seed,
    };

    client
        .program
        .request()
        .accounts(accounts)
        .args(instruction)
        .signer(user_wallet.as_ref())
        .send_with_spinner_and_config(Default::default())
        .context("Failed to send stack deletion transaction")?;

    Ok(())
}
