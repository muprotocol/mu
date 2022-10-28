use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount, Transfer};

declare_id!("2MZLka8nfoAf1LKCCbgCw5ZXfpMbKGDuLjQ88MNMyti2");

fn calc_usage(rates: &ServiceUnits, usage: &ServiceUnits) -> u64 {
    rates.bandwidth * usage.bandwidth
        + rates.gateway_mreqs * usage.gateway_mreqs
        + rates.mudb_gb_month * usage.mudb_gb_month
        + rates.mufunction_cpu_mem * usage.mufunction_cpu_mem
}

pub enum MuAccountType {
    MuState = 0,
    Provider = 1,
    ProviderRegion = 2,
    UsageUpdate = 3,
    AuthorizedUsageSigner = 4,
    Stack = 5,
}

#[program]
pub mod marketplace {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.state.set_inner(MuState {
            account_type: MuAccountType::MuState as u8,
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

        ctx.accounts.provider.set_inner(Provider {
            account_type: MuAccountType::Provider as u8,
            name,
            owner: ctx.accounts.owner.key(),
            bump: *ctx.bumps.get("provider").unwrap(),
        });

        Ok(())
    }

    pub fn create_stack(
        ctx: Context<CreateStack>,
        stack_seed: u64,
        stack_data: Vec<u8>,
    ) -> Result<()> {
        ctx.accounts.stack.set_inner(Stack {
            account_type: MuAccountType::Stack as u8,
            stack: stack_data,
            user: ctx.accounts.user.key(),
            region: ctx.accounts.region.key(),
            seed: stack_seed,
            revision: 1,
            bump: *ctx.bumps.get("stack").unwrap(),
        });

        Ok(())
    }

    pub fn create_region(
        ctx: Context<CreateRegion>,
        region_num: u32,
        name: String,
        zones: u8,
        rates: ServiceUnits,
    ) -> Result<()> {
        ctx.accounts.region.set_inner(ProviderRegion {
            account_type: MuAccountType::ProviderRegion as u8,
            name,
            zones,
            region_num,
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
                account_type: MuAccountType::AuthorizedUsageSigner as u8,
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
        let bump = ctx.accounts.state.bump.to_le_bytes();
        let pito = vec![b"state".as_ref(), bump.as_ref()];
        let outpito = vec![pito.as_slice()];
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer,
            &outpito.as_slice(),
        );
        anchor_spl::token::transfer(transfer_ctx, usage_tokens)?;

        ctx.accounts.usage_update.set_inner(UsageUpdate {
            account_type: MuAccountType::UsageUpdate as u8,
            region: ctx.accounts.region.key(),
            stack: ctx.accounts.stack.key(),
            usage,
        });

        Ok(())
    }
}

#[account]
#[derive(Default)]
pub struct MuState {
    pub account_type: u8, // See MuAccountType
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub deposit_token: Pubkey,
    pub bump: u8,
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

#[account]
pub struct Provider {
    pub account_type: u8, // See MuAccountType
    pub name: String,
    pub owner: Pubkey,
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
        space = 8 + 1 + 4 + name.as_bytes().len() + 32 + 1,
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

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ServiceUnits {
    pub mudb_gb_month: u64,
    pub mufunction_cpu_mem: u64,
    pub bandwidth: u64,
    pub gateway_mreqs: u64,
}

#[account]
pub struct ProviderRegion {
    pub account_type: u8, // See MuAccountType
    pub provider: Pubkey,
    pub zones: u8,
    pub region_num: u32,
    pub rates: ServiceUnits,
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
        space = 8 + 1 + 32 + 4 + name.as_bytes().len() + 1 + 4 + (8 + 8 + 8 + 8) + 1,
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

#[account]
pub struct Stack {
    pub account_type: u8, // See MuAccountType
    pub user: Pubkey,
    pub region: Pubkey,
    pub stack: Vec<u8>,
    pub seed: u64,
    pub revision: u32,
    pub bump: u8,
}

#[derive(Accounts)]
#[instruction(stack_seed: u64, stack_data: Vec<u8>)]
pub struct CreateStack<'info> {
    pub region: Account<'info, ProviderRegion>,

    #[account(
        init,
        payer = user,
        space = 8 + 1 + 32 + 32 + 4 + stack_data.len() + 8 + 4 + 1,
        seeds = [b"stack", user.key().as_ref(), region.key().as_ref(), stack_seed.to_le_bytes().as_ref()],
        bump
    )]
    pub stack: Account<'info, Stack>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct AuthorizedUsageSigner {
    pub account_type: u8, // See MuAccountType
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
        space = 8 + 1 + 32 + 32,
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
    pub account_type: u8, // See MuAccountType
    pub region: Pubkey,
    pub stack: Pubkey,
    pub usage: ServiceUnits,
}

#[derive(Accounts)]
#[instruction(update_seed: u64, escrow_bump: u8)]
pub struct UpdateUsage<'info> {
    #[account(
        seeds = [b"state"],
        bump = state.bump
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

    // TODO: add the developer's account as input, calculate and validate the stack's PDA
    #[account(has_one = region)]
    stack: Account<'info, Stack>,

    #[account(mut)]
    signer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
