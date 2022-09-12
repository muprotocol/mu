use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount, Transfer};

declare_id!("9YNk2WSJ4rXsvJFM2teXJhJ2LZL3XDUPP2FAiNuGsFtx");

#[program]
pub mod marketplace {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.state.set_inner(MuState::new(
            ctx.accounts.authority.key(),
            ctx.accounts.mint.key(),
            ctx.accounts.deposit_token.key(),
            *ctx.bumps.get("state").unwrap(),
        ));

        Ok(())
    }

    pub fn create_provider(ctx: Context<CreateProvider>, name: String) -> Result<()> {
        let transfer = Transfer {
            from: ctx.accounts.owner_token.to_account_info(),
            to: ctx.accounts.deposit_token.to_account_info(),
            authority: ctx.accounts.owner.to_account_info(),
        };
        let transfer_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), transfer);
        anchor_spl::token::transfer(transfer_ctx, 100)?;

        ctx.accounts.provider.set_inner(Provider::new(
            name,
            ctx.accounts.owner.key(),
            ctx.accounts.owner_token.key(),
            *ctx.bumps.get("provider").unwrap(),
        ));

        Ok(())
    }

    pub fn create_stack(
        ctx: Context<CreateStack>,
        _stack_size: u32,
        _stack_seed: u64,
        stack: Vec<u8>,
    ) -> Result<()> {
        ctx.accounts.stack.set_inner(Stack::new(
            stack,
            ctx.accounts.owner.key(),
            ctx.accounts.region.key(),
        ));

        Ok(())
    }

    pub fn create_region(
        ctx: Context<CreateRegion>,
        _region_num: u8,
        name: String,
        zones: u8,
        rates: ServiceRates,
    ) -> Result<()> {
        ctx.accounts.region.set_inner(ProviderRegion::new(
            name,
            zones,
            rates,
            ctx.accounts.provider.key(),
            *ctx.bumps.get("region").unwrap(),
        ));

        Ok(())
    }

    pub fn create_provider_escrow_account(
        _ctx: Context<CreateProviderEscrowAccount>,
    ) -> Result<()> {
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ServiceRates {
    mudb_gb_month: u64,
    mufunction_cpu_mem: u64,
    bandwidth: u64,
    gateway_mreqs: u64,
}

#[account]
#[derive(Default)]
pub struct MuState {
    account_type: u8, // Always 0
    authority: Pubkey,
    mint: Pubkey,
    deposit_token: Pubkey,
    bump: u8,
}

impl MuState {
    pub fn new(authority: Pubkey, mint: Pubkey, deposit_token: Pubkey, bump: u8) -> MuState {
        MuState {
            account_type: 0,
            authority,
            mint,
            deposit_token,
            bump,
        }
    }
}

#[account]
pub struct Provider {
    account_type: u8, // Always 1
    name: String,     // Max 20 Chars
    owner: Pubkey,
    owner_token: Pubkey,
    bump: u8,
}

impl Provider {
    pub fn new(name: String, owner: Pubkey, owner_token: Pubkey, bump: u8) -> Provider {
        Provider {
            account_type: 1,
            name,
            owner,
            owner_token,
            bump,
        }
    }
}

#[account]
pub struct Stack {
    account_type: u8, // Always 2
    owner: Pubkey,
    region: Pubkey,
    stack: Vec<u8>,
}

impl Stack {
    pub fn new(stack: Vec<u8>, owner: Pubkey, region: Pubkey) -> Stack {
        Stack {
            account_type: 2,
            stack,
            owner,
            region,
        }
    }
}

#[account]
pub struct ProviderRegion {
    account_type: u8, // Always 3
    provider: Pubkey,
    name: String, // Max 20
    zones: u8,
    rates: ServiceRates,
    bump: u8,
}

impl ProviderRegion {
    pub fn new(
        name: String,
        zones: u8,
        rates: ServiceRates,
        provider: Pubkey,
        bump: u8,
    ) -> ProviderRegion {
        ProviderRegion {
            account_type: 3,
            name,
            zones,
            rates,
            provider,
            bump,
        }
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        seeds = [b"state"],
        space = 1000,
        bump
    )]
    state: Account<'info, MuState>,

    mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        token::mint = mint,
        token::authority = state,
        seeds = [b"deposit"],
        bump
    )]
    pub deposit_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct CreateProvider<'info> {
    #[account(
        seeds = [b"state"],
        bump = state.bump,
        has_one = deposit_token
    )]
    state: Account<'info, MuState>,

    #[account(
        init,
        payer = owner,
        space = 8 + 1 + 20 + 32 + 32 + 1,
        seeds = [b"provider", owner.key().as_ref()],
        bump
    )]
    pub provider: Account<'info, Provider>,

    #[account(mut)]
    pub deposit_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub owner_token: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(region_num: u8)]
pub struct CreateRegion<'info> {
    #[account(has_one = owner)]
    pub provider: Account<'info, Provider>,

    #[account(
        init,
        space = 8 + 20 + 1 + (8 + 8 + 8 + 8) + 32 + 1,
        payer = owner,
        seeds = [b"region", owner.key().as_ref(), region_num.to_be_bytes().as_ref()],
        bump
    )]
    pub region: Account<'info, ProviderRegion>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateProviderEscrowAccount<'info> {
    #[account(has_one = mint)]
    pub state: Account<'info, MuState>,
    pub mint: Account<'info, Mint>,

    #[account(
        init,
        seeds = [b"escrow", owner.key().as_ref(), provider.key().as_ref()],
        payer = owner,
        token::mint = mint,
        token::authority = state,
        bump
    )]
    pub escrow_account: Account<'info, TokenAccount>,

    pub provider: Account<'info, Provider>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(stack_size: u32, stack_seed: u64)]
pub struct CreateStack<'info> {
    pub region: Account<'info, ProviderRegion>,

    #[account(
        init,
        payer = owner,
        space = 8 + 1 + 32 + 32 + 4 + stack_size as usize,
        seeds = [b"stack", owner.key().as_ref(), region.key().as_ref(), stack_seed.to_be_bytes().as_ref()],
        bump
    )]
    pub stack: Account<'info, Stack>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
