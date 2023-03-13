use anchor_client::anchor_lang::AccountDeserialize;
use anyhow::{bail, Context, Result};
use solana_account_decoder::parse_token::{parse_token, TokenAccountType};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{account::ReadableAccount, pubkey::Pubkey};

pub async fn get_token_decimals(rpc_client: &RpcClient, marketplace_id: &Pubkey) -> Result<u8> {
    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], marketplace_id);
    let state = rpc_client
        .get_account(&state_pda)
        .await
        .context("Failed to fetch mu state from Solana")?;
    let state = marketplace::MuState::try_deserialize(&mut state.data())
        .context("Failed to read mu state from Solana")?;

    let mint_address = state.mint;
    let mint = rpc_client
        .get_account(&mint_address)
        .await
        .context("Failed to fetch $MU mint from Solana")?;
    let mint = parse_token(mint.data(), None).context("Failed to read $MU mint from Solana")?;

    if let TokenAccountType::Mint(mint) = mint {
        Ok(mint.decimals)
    } else {
        bail!("Expected $MU mint to be a mint account");
    }
}
