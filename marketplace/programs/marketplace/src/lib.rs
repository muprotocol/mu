// We have to use anchor's error type, we have no control over it
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount, Transfer};

declare_id!("2MZLka8nfoAf1LKCCbgCw5ZXfpMbKGDuLjQ88MNMyti2");

fn calc_usage(rates: &ServiceRates, usage: &ServiceUsage) -> u64 {
    (rates.function_mb_tera_instructions as u128 * usage.function_mb_instructions / 1_000_000)
        as u64
        + (rates.db_gigabyte_months as u128 * usage.db_bytes_seconds
            / (1024 * 1024 * 1024 * 60 * 60 * 24 * 30)) as u64
        + (rates.million_db_reads * usage.db_reads / 1_000_000)
        + (rates.million_db_writes * usage.db_writes / 1_000_000)
        + (rates.million_gateway_requests * usage.gateway_requests / 1_000_000)
        + (rates.gigabytes_gateway_traffic * usage.gateway_traffic_bytes / (1024 * 1024 * 1024))
}

#[error_code]
pub enum Error {
    #[msg("Provider is not authorized")]
    ProviderNotAuthorized,

    #[msg("Commission rate is out of bounds")]
    CommissionRateOutOfBounds,

    #[msg("Cannot operate on a deleted stack")]
    CannotOperateOnDeletedStack,
}

#[program]
pub mod marketplace {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, commission_rate_micros: u32) -> Result<()> {
        if commission_rate_micros > 1_000_000 {
            return Err(Error::CommissionRateOutOfBounds.into());
        }

        ctx.accounts.state.set_inner(MuState {
            authority: ctx.accounts.authority.key(),
            mint: ctx.accounts.mint.key(),
            deposit_token: ctx.accounts.deposit_token.key(),
            commission_token: ctx.accounts.commission_token.key(),
            commission_rate_micros,
            bump: *ctx.bumps.get("state").unwrap(),
        });

        Ok(())
    }

    pub fn create_provider_authorizer(ctx: Context<CreateProviderAuthorizer>) -> Result<()> {
        ctx.accounts
            .provider_authorizer
            .set_inner(ProviderAuthorizer {
                authorizer: ctx.accounts.authorizer.key(),
                bump: *ctx.bumps.get("provider_authorizer").unwrap(),
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
        anchor_spl::token::transfer(transfer_ctx, 100_000000)?; // TODO: make this configurable

        ctx.accounts.provider.set_inner(Provider {
            name,
            authorized: false,
            owner: ctx.accounts.owner.key(),
            bump: *ctx.bumps.get("provider").unwrap(),
        });

        Ok(())
    }

    pub fn authorize_provider(ctx: Context<AuthorizeProvider>) -> Result<()> {
        ctx.accounts.provider.authorized = true;
        Ok(())
    }

    pub fn create_region(
        ctx: Context<CreateRegion>,
        region_num: u32,
        name: String,
        rates: ServiceRates,
        min_escrow_balance: u64,
    ) -> Result<()> {
        if !ctx.accounts.provider.authorized {
            return Err(Error::ProviderNotAuthorized.into());
        }

        ctx.accounts.region.set_inner(ProviderRegion {
            name,
            region_num,
            rates,
            min_escrow_balance,
            provider: ctx.accounts.provider.key(),
            bump: *ctx.bumps.get("region").unwrap(),
        });

        Ok(())
    }

    pub fn create_stack(
        ctx: Context<CreateStack>,
        stack_seed: u64,
        stack_data: Vec<u8>,
        name: String,
    ) -> Result<()> {
        if !ctx.accounts.provider.authorized {
            return Err(Error::ProviderNotAuthorized.into());
        }

        ctx.accounts.stack.set_inner(Stack {
            user: ctx.accounts.user.key(),
            region: ctx.accounts.region.key(),
            seed: stack_seed,
            bump: *ctx.bumps.get("stack").unwrap(),
            state: StackState::Active {
                revision: 1,
                name,
                stack_data,
            },
        });

        Ok(())
    }

    pub fn update_stack(
        ctx: Context<UpdateStack>,
        _stack_seed: u64,
        stack_data: Vec<u8>,
        name: String,
    ) -> Result<()> {
        match ctx.accounts.stack.state {
            StackState::Deleted => Err(Error::CannotOperateOnDeletedStack.into()),
            StackState::Active {
                ref mut revision,
                name: ref mut name_ref,
                stack_data: ref mut stack_data_ref,
            } => {
                *name_ref = name;
                *stack_data_ref = stack_data;
                *revision += 1;

                Ok(())
            }
        }
    }

    pub fn delete_stack(ctx: Context<DeleteStack>, _stack_seed: u64) -> Result<()> {
        if let StackState::Deleted = ctx.accounts.stack.state {
            return Err(Error::CannotOperateOnDeletedStack.into());
        }

        ctx.accounts.stack.state = StackState::Deleted;
        Ok(())
    }

    pub fn create_authorized_usage_signer(
        ctx: Context<CreateAuthorizedUsageSigner>,
        // TODO: why aren't these in the Accounts struct?
        signer: Pubkey,
        token_account: Pubkey,
    ) -> Result<()> {
        ctx.accounts
            .authorized_signer
            .set_inner(AuthorizedUsageSigner {
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

    pub fn withdraw_escrow_balance(ctx: Context<WithdrawEscrow>, amount: u64) -> Result<()> {
        let bump = ctx.accounts.state.bump.to_le_bytes();
        let signer_seeds = vec![b"state".as_ref(), bump.as_ref()];
        let signer_seeds_wrapper = vec![signer_seeds.as_slice()];

        let transfer = Transfer {
            from: ctx.accounts.escrow_account.to_account_info(),
            to: ctx.accounts.withdraw_to.to_account_info(),
            authority: ctx.accounts.state.to_account_info(),
        };
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer,
            signer_seeds_wrapper.as_slice(),
        );
        anchor_spl::token::transfer(transfer_ctx, amount)?;

        Ok(())
    }

    pub fn update_usage(
        ctx: Context<UpdateUsage>,
        update_seed: u128,
        _escrow_bump: u8,
        usage: ServiceUsage,
    ) -> Result<()> {
        // TODO: only allow usage updates up to a certain point in time after the stack was deleted
        let usage_tokens = calc_usage(&ctx.accounts.region.rates, &usage);
        let commission_tokens =
            usage_tokens * ctx.accounts.state.commission_rate_micros as u64 / 1_000_000;
        let provider_tokens = usage_tokens - commission_tokens;
        msg!(
            "Calculated price: {}, commission: {}, provider's share: {}",
            usage_tokens,
            commission_tokens,
            provider_tokens,
        );

        let bump = ctx.accounts.state.bump.to_le_bytes();
        let signer_seeds = vec![b"state".as_ref(), bump.as_ref()];
        let signer_seeds_wrapper = vec![signer_seeds.as_slice()];

        let transfer = Transfer {
            from: ctx.accounts.escrow_account.to_account_info(),
            to: ctx.accounts.token_account.to_account_info(),
            authority: ctx.accounts.state.to_account_info(),
        };
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer,
            signer_seeds_wrapper.as_slice(),
        );
        anchor_spl::token::transfer(transfer_ctx, provider_tokens)?;

        let transfer = Transfer {
            from: ctx.accounts.escrow_account.to_account_info(),
            to: ctx.accounts.commission_token.to_account_info(),
            authority: ctx.accounts.state.to_account_info(),
        };
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer,
            signer_seeds_wrapper.as_slice(),
        );
        anchor_spl::token::transfer(transfer_ctx, commission_tokens)?;

        ctx.accounts.usage_update.set_inner(UsageUpdate {
            region: ctx.accounts.region.key(),
            stack: ctx.accounts.stack.key(),
            seed: update_seed,
            usage,
        });

        Ok(())
    }
}

#[account]
#[derive(Default)]
pub struct MuState {
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub deposit_token: Pubkey,
    pub commission_token: Pubkey,
    pub commission_rate_micros: u32,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        seeds = [b"state"],
        space = 8 + 32 + 32 + 32 + 32 + 4 + 1,
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

    #[account(
        init,
        payer = authority,
        token::mint = mint,
        token::authority = state,
        seeds = [b"commission"],
        bump
    )]
    pub commission_token: Account<'info, TokenAccount>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[account]
#[derive(Default)]
pub struct ProviderAuthorizer {
    pub authorizer: Pubkey,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct CreateProviderAuthorizer<'info> {
    #[account(
        seeds = [b"state"],
        bump = state.bump,
        has_one = authority
    )]
    state: Account<'info, MuState>,

    #[account(
        init,
        payer = authority,
        seeds = [b"authorizer", authorizer.key().as_ref()],
        space = 8 + 32 + 1,
        bump,
    )]
    pub provider_authorizer: Account<'info, ProviderAuthorizer>,

    #[account(mut)]
    pub authority: Signer<'info>,

    // Require a signature from the authorizer as well to ensure its private key
    // is available somewhere.
    pub authorizer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[account]
pub struct Provider {
    pub owner: Pubkey,
    pub authorized: bool,
    pub name: String,
    pub bump: u8,
}

#[derive(Accounts)]
#[instruction(name: String)]
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
        space = 8 + 32 + 1 + 4 + name.as_bytes().len() + 1,
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
pub struct AuthorizeProvider<'info> {
    #[account(
        seeds = [b"authorizer", authorizer.key().as_ref()],
        bump = provider_authorizer.bump,
        has_one = authorizer
    )]
    provider_authorizer: Account<'info, ProviderAuthorizer>,

    pub authorizer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"provider", owner.key().as_ref()],
        bump = provider.bump
    )]
    pub provider: Account<'info, Provider>,

    /// CHECK: The provider's wallet, used only to validate the provider account's PDA
    pub owner: AccountInfo<'info>,
}

// This is essentially the same data as in ServiceUsage, but with
// units that make more sense for pricing.
// The prices are in token amount *without* floating point, so
// a price of 100, when the $MU token has 4 decimal places, is
// actually 0.01 $MU.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ServiceRates {
    pub function_mb_tera_instructions: u64,
    pub db_gigabyte_months: u64,
    pub million_db_reads: u64,
    pub million_db_writes: u64,
    pub million_gateway_requests: u64,
    pub gigabytes_gateway_traffic: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct ServiceUsage {
    pub function_mb_instructions: u128, // TODO: should we round a few zeroes off the instruction count?
    pub db_bytes_seconds: u128,
    pub db_reads: u64,
    pub db_writes: u64,
    pub gateway_requests: u64,
    pub gateway_traffic_bytes: u64,
}

#[account]
pub struct ProviderRegion {
    pub provider: Pubkey,
    pub region_num: u32,
    pub rates: ServiceRates,
    pub min_escrow_balance: u64,
    pub bump: u8,
    pub name: String,
}

#[derive(Accounts)]
#[instruction(region_num: u32, name: String)]
pub struct CreateRegion<'info> {
    #[account(has_one = owner)]
    pub provider: Account<'info, Provider>,

    #[account(
        init,
        space = 8 + 32 + 4 + (8 + 8 + 8 + 8 + 8 + 8) + 8 + 1 + 4 + name.as_bytes().len(),
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
    #[account(
        seeds = [b"state"],
        bump = state.bump,
        has_one = mint
    )]
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
pub struct WithdrawEscrow<'info> {
    #[account(
        seeds = [b"state"],
        bump = state.bump,
    )]
    pub state: Account<'info, MuState>,

    #[account(
        seeds = [b"escrow", user.key().as_ref(), provider.key().as_ref()],
        bump,
        mut
    )]
    pub escrow_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub withdraw_to: Account<'info, TokenAccount>,

    pub provider: Account<'info, Provider>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[account]
pub struct Stack {
    pub user: Pubkey,
    pub region: Pubkey,
    pub seed: u64,
    pub bump: u8,
    pub state: StackState,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum StackState {
    // I hate putting so many fields in an enum case, but if we declare a struct
    // for the fields, the anchor TS client library will choke on it.
    Active {
        revision: u32,
        name: String,
        stack_data: Vec<u8>,
    },
    Deleted,
}

#[repr(u8)]
pub enum StackStateDiscriminator {
    Active = 0,
    Deleted = 1,
}

#[derive(Accounts)]
#[instruction(stack_seed: u64, stack_data: Vec<u8>, name: String)]
pub struct CreateStack<'info> {
    pub provider: Account<'info, Provider>,

    #[account(has_one = provider)]
    pub region: Account<'info, ProviderRegion>,

    #[account(
        init,
        payer = user,
        space = 8 + 32 + 32 + 8 + 1 + 1 + 4 + 4 + name.len() + 4 + stack_data.len(),
        seeds = [b"stack", user.key().as_ref(), region.key().as_ref(), stack_seed.to_le_bytes().as_ref()],
        bump
    )]
    pub stack: Account<'info, Stack>,

    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(stack_seed: u64, stack_data: Vec<u8>, name: String)]
pub struct UpdateStack<'info> {
    pub region: Account<'info, ProviderRegion>,

    #[account(
        mut,
        realloc = 8 + 32 + 32 + 8 + 1 + 1 + 4 + 4 + name.len() + 4 + stack_data.len(),
        realloc::payer = user,
        realloc::zero = false,
        seeds = [b"stack", user.key().as_ref(), region.key().as_ref(), stack_seed.to_le_bytes().as_ref()],
        has_one = user,
        bump = stack.bump,
    )]
    pub stack: Account<'info, Stack>,

    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(stack_seed: u64)]
pub struct DeleteStack<'info> {
    pub region: Account<'info, ProviderRegion>,

    #[account(
        mut,
        realloc = 8 + 32 + 32 + 8 + 1 + 1,
        realloc::payer = user,
        realloc::zero = false,
        seeds = [b"stack", user.key().as_ref(), region.key().as_ref(), stack_seed.to_le_bytes().as_ref()],
        has_one = user,
        bump = stack.bump,
    )]
    pub stack: Account<'info, Stack>,

    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct AuthorizedUsageSigner {
    pub signer: Pubkey,
    pub token_account: Pubkey,
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
        space = 8 + 32 + 32,
        seeds = [b"authorized_signer", region.key().as_ref()],
        bump
    )]
    authorized_signer: Account<'info, AuthorizedUsageSigner>,

    #[account(mut)]
    owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[account]
pub struct UsageUpdate {
    pub region: Pubkey,
    pub stack: Pubkey,
    pub seed: u128,
    pub usage: ServiceUsage,
}

#[derive(Accounts)]
#[instruction(update_seed: u128, escrow_bump: u8)]
pub struct UpdateUsage<'info> {
    #[account(
        seeds = [b"state"],
        bump = state.bump,
        has_one = commission_token
    )]
    pub state: Account<'info, MuState>,

    /// CHECK: The commission token account as verified by has_one on state
    #[account(mut)]
    commission_token: AccountInfo<'info>,

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
        space = 8 + 32 + 32 + 16 + (16 + 16 + 8 + 8 + 8 + 8),
        seeds = [
            b"update",
            stack.key().as_ref(),
            region.key().as_ref(),
            update_seed.to_le_bytes().as_ref()
        ],
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

    // TODO: add the developer's account as input, calculate and validate the stack's PDA
    #[account(has_one = region)]
    stack: Account<'info, Stack>,

    #[account(mut)]
    signer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
