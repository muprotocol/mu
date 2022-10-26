use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount, Transfer};

declare_id!("2MZLka8nfoAf1LKCCbgCw5ZXfpMbKGDuLjQ88MNMyti2");

#[error_code]
pub enum Errors {
    #[msg("Name can't be more than 20 chars")]
    NameTooLong,
}

fn calc_usage(rates: &ServiceUnits, usage: &ServiceUnits) -> u64 {
    rates.bandwidth * usage.bandwidth
        + rates.gateway_mreqs * usage.gateway_mreqs
        + rates.mudb_gb_month * usage.mudb_gb_month
        + rates.mufunction_cpu_mem * usage.mufunction_cpu_mem
}

#[program]
pub mod marketplace {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.state.set_inner(MuState {
            account_type: 0,
            authority: ctx.accounts.authority.key(),
            mint: ctx.accounts.mint.key(),
            deposit_token: ctx.accounts.deposit_token.key(),
            bump: *ctx.bumps.get("state").unwrap(),
        });

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

        if name.len() > 20 {
            return err!(Errors::NameTooLong);
        }

        ctx.accounts.provider.set_inner(Provider {
            account_type: 1,
            name,
            owner: ctx.accounts.owner.key(),
            bump: *ctx.bumps.get("provider").unwrap(),
        });

        Ok(())
    }

    pub fn create_stack(
        ctx: Context<CreateStack>,
        _stack_size: u32,
        _stack_seed: u64,
        stack: Vec<u8>,
    ) -> Result<()> {
        ctx.accounts.stack.set_inner(Stack {
            account_type: 2,
            stack,
            user: ctx.accounts.user.key(),
            region: ctx.accounts.region.key(),
        });

        Ok(())
    }

    pub fn create_region(
        ctx: Context<CreateRegion>,
        _region_num: u8,
        name: String,
        zones: u8,
        rates: ServiceUnits,
    ) -> Result<()> {
        ctx.accounts.region.set_inner(ProviderRegion {
            account_type: 3,
            name,
            zones,
            rates,
            provider: ctx.accounts.provider.key(),
            bump: *ctx.bumps.get("region").unwrap(),
        });

        Ok(())
    }

    pub fn create_authorized_usage_signer(
        ctx: Context<CreateAuthorizedUsageSigner>,
        signer: Pubkey,
        token_account: Pubkey,
    ) -> Result<()> {
        ctx.accounts
            .authorized_signer
            .set_inner(AuthorizedUsageSigner {
                account_type: 4,
                signer,
                token_account,
            });

        Ok(())
    }

    pub fn create_provider_escrow_account(
        _ctx: Context<CreateProviderEscrowAccount>,
    ) -> Result<()> {
        Ok(())
    }

    pub fn update_usage(
        ctx: Context<UpdateUsage>,
        _update_seed: u64,
        _escrow_bump: u8,
        usage: ServiceUnits,
    ) -> Result<()> {
        let usage_tokens = calc_usage(&ctx.accounts.region.rates, &usage);
        let transfer = Transfer {
            from: ctx.accounts.escrow_account.to_account_info(),
            to: ctx.accounts.token_account.to_account_info(),
            authority: ctx.accounts.state.to_account_info(),
        };
        let bump = ctx.bumps.get("state").unwrap().to_le_bytes();
        let pito = vec![b"state".as_ref(), bump.as_ref()];
        let outpito = vec![pito.as_slice()];
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer,
            &outpito.as_slice(),
        );
        anchor_spl::token::transfer(transfer_ctx, usage_tokens)?;

        // ctx.accounts.usage_update.set_inner(UsageUpdate {
        //     account_type: 4,
        //     region: ctx.accounts.region.key(),
        //     stack: ctx.accounts.stack.key(),
        //     usage,
        // });

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ServiceUnits {
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

#[account]
pub struct Provider {
    account_type: u8, // Always 1
    name: String,     // Max 20 Chars
    owner: Pubkey,
    bump: u8,
}

#[account]
pub struct ProviderRegion {
    account_type: u8, // Always 2
    provider: Pubkey,
    name: String, // Max 20
    zones: u8,
    rates: ServiceUnits,
    bump: u8,
}

#[account]
pub struct UsageUpdate {
    account_type: u8, // Always 3
    region: Pubkey,
    stack: Pubkey,
    usage: ServiceUnits,
}

#[account]
pub struct AuthorizedUsageSigner {
    account_type: u8,
    signer: Pubkey,
    token_account: Pubkey,
}

#[account]
pub struct Stack {
    account_type: u8, // Always 4
    user: Pubkey,
    region: Pubkey,
    stack: Vec<u8>,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        seeds = [b"state"],
        space = 8 + 1 + 32 + 32 + 32 + 1,
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
        seeds = [b"region", owner.key().as_ref(), region_num.to_le_bytes().as_ref()],
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
        seeds = [b"escrow", user.key().as_ref(), provider.key().as_ref()],
        payer = user,
        token::mint = mint,
        token::authority = state,
        bump
    )]
    pub escrow_account: Account<'info, TokenAccount>,

    pub provider: Account<'info, Provider>,

    #[account(mut)]
    pub user: Signer<'info>,

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
        payer = user,
        space = 8 + 1 + 32 + 32 + 4 + stack_size as usize,
        seeds = [b"stack", user.key().as_ref(), region.key().as_ref(), stack_seed.to_le_bytes().as_ref()],
        bump
    )]
    pub stack: Account<'info, Stack>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CreateAuthorizedUsageSigner<'info> {
    #[account(
        has_one = owner
    )]
    provider: Account<'info, Provider>,

    #[account(
        has_one = provider,
    )]
    region: Account<'info, ProviderRegion>,

    #[account(
        init,
        payer = owner,
        space = 8 + 1 + 32 + 32,
        seeds = [b"authorized_signer", region.key().as_ref()],
        bump
    )]
    authorized_signer: Account<'info, AuthorizedUsageSigner>,

    #[account(mut)]
    owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(update_seed: u64, escrow_bump: u8)]
pub struct UpdateUsage<'info> {
    #[account(
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, MuState>,

    #[account(
        has_one = signer,
        has_one = token_account,
        seeds = [b"authorized_signer", region.key().as_ref()],
        bump
    )]
    authorized_signer: Account<'info, AuthorizedUsageSigner>,

    pub region: Account<'info, ProviderRegion>,

    /// CHECK: The token account for the provider
    #[account(mut)]
    token_account: AccountInfo<'info>,

    #[account(
        init,
        payer = signer,
        space = 8 + 1 + 32 + 32 + (8 + 8 + 8 + 8),
        seeds = [b"update", update_seed.to_le_bytes().as_ref()],
        bump
    )]
    usage_update: Account<'info, UsageUpdate>,
    /// CHECK: The escrow account for the deposits
    #[account(
        mut,
        seeds = [b"escrow", stack.user.key().as_ref(), region.provider.key().as_ref()],
        bump = escrow_bump
    )]
    escrow_account: AccountInfo<'info>,

    #[account(has_one = region)]
    stack: Account<'info, Stack>,

    #[account(mut)]
    signer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
