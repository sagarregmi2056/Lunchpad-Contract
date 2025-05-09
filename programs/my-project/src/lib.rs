use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, MintTo, Burn};
use anchor_lang::system_program::{self, Transfer};

declare_id!("ExiyW5RS1e4XxjxeZHktijRhnYF6sJYzfmdzU85gFbS4");

#[program]
pub mod bonding_curve_new {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        initial_price: u64,
        slope: u64,
    ) -> Result<()> {
        if initial_price == 0 || slope == 0 {
            return Err(ErrorCode::InvalidParameters.into());
        }

        let bonding_curve = &mut ctx.accounts.bonding_curve;

        bonding_curve.authority = ctx.accounts.authority.key();
        bonding_curve.initial_price = initial_price;
        bonding_curve.slope = slope;
        bonding_curve.total_supply = 0;
        bonding_curve.token_mint = ctx.accounts.token_mint.key();
        bonding_curve.bump = ctx.bumps.bonding_curve;

        // Log the parameters
        msg!("Bonding curve initialized with initial price: {} lamports, slope: {} lamports", 
            initial_price, slope);

        Ok(())
    }

    pub fn update_parameters(
        ctx: Context<UpdateParameters>,
        initial_price: u64,
        slope: u64,
    ) -> Result<()> {
        if initial_price == 0 || slope == 0 {
            return Err(ErrorCode::InvalidParameters.into());
        }

        let bonding_curve = &mut ctx.accounts.bonding_curve;

        // Only the authority can update parameters
        if bonding_curve.authority != ctx.accounts.authority.key() {
            return Err(ErrorCode::Unauthorized.into());
        }

        // Store the old values for logging
        let old_initial_price = bonding_curve.initial_price;
        let old_slope = bonding_curve.slope;

        // Update the parameters
        bonding_curve.initial_price = initial_price;
        bonding_curve.slope = slope;

        // Log the changes
        msg!("Bonding curve parameters updated: initial_price {} -> {}, slope {} -> {}", 
            old_initial_price, initial_price, old_slope, slope);

        Ok(())
    }

    pub fn buy_tokens(
        ctx: Context<BuyTokens>,
        amount: u64,
    ) -> Result<()> {
        if amount == 0 {
            return Err(ErrorCode::InvalidAmount.into());
        }

        let bonding_curve = &ctx.accounts.bonding_curve;

        if bonding_curve.authority != ctx.accounts.authority.key() {
            return Err(ErrorCode::Unauthorized.into());
        }

        let current_price = bonding_curve.initial_price
            .checked_add(bonding_curve.total_supply
                .checked_mul(bonding_curve.slope)
                .ok_or(ErrorCode::Overflow)?)
            .ok_or(ErrorCode::Overflow)?;

        let cost = current_price
            .checked_mul(amount)
            .ok_or(ErrorCode::Overflow)?;

        // Log the purchase details
        msg!("Buying {} tokens at {} lamports per token. Total cost: {} lamports", 
            amount, current_price, cost);

        // Transfer SOL from buyer to bonding curve account
        let transfer_ctx = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.buyer.to_account_info(),
                to: ctx.accounts.bonding_curve.to_account_info(),
            },
        );
        system_program::transfer(transfer_ctx, cost)?;

        // Mint tokens to buyer using the token mint in the seeds
        let token_mint_key = ctx.accounts.token_mint.key();
        let seeds = &[
            b"bonding_curve".as_ref(),
            token_mint_key.as_ref(),
            &[bonding_curve.bump]
        ];
        let signer = &[&seeds[..]];

        let mint_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.token_mint.to_account_info(),
                to: ctx.accounts.buyer_token_account.to_account_info(),
                authority: ctx.accounts.bonding_curve.to_account_info(),
            },
            signer,
        );
        token::mint_to(mint_ctx, amount)?;

        // Update state
        let bonding_curve = &mut ctx.accounts.bonding_curve;
        bonding_curve.total_supply = bonding_curve.total_supply
            .checked_add(amount)
            .ok_or(ErrorCode::Overflow)?;

        Ok(())
    }

    pub fn sell_tokens(
        ctx: Context<SellTokens>,
        amount: u64,
    ) -> Result<()> {
        if amount == 0 {
            return Err(ErrorCode::InvalidAmount.into());
        }

        let bonding_curve = &ctx.accounts.bonding_curve;

        if bonding_curve.authority != ctx.accounts.authority.key() {
            return Err(ErrorCode::Unauthorized.into());
        }

        if ctx.accounts.seller_token_account.owner != ctx.accounts.seller.key()
            || ctx.accounts.seller_token_account.mint != ctx.accounts.token_mint.key() {
            return Err(ErrorCode::InvalidTokenAccount.into());
        }

        let current_price = bonding_curve.initial_price
            .checked_add(bonding_curve.total_supply
                .checked_mul(bonding_curve.slope)
                .ok_or(ErrorCode::Overflow)?)
            .ok_or(ErrorCode::Overflow)?;

        let refund = current_price
            .checked_mul(amount)
            .ok_or(ErrorCode::Overflow)?;

        // Log the sell details
        msg!("Selling {} tokens at {} lamports per token. Total refund: {} lamports", 
            amount, current_price, refund);

        // Burn tokens from seller using the token mint in the seeds
        let token_mint_key = ctx.accounts.token_mint.key();
        let seeds = &[
            b"bonding_curve".as_ref(),
            token_mint_key.as_ref(),
            &[bonding_curve.bump]
        ];
        let signer = &[&seeds[..]];

        let burn_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.token_mint.to_account_info(),
                from: ctx.accounts.seller_token_account.to_account_info(),
                authority: ctx.accounts.bonding_curve.to_account_info(),
            },
            signer,
        );
        token::burn(burn_ctx, amount)?;

        // Send SOL back to seller
        **ctx.accounts.bonding_curve.to_account_info().try_borrow_mut_lamports()? -= refund;
        **ctx.accounts.seller.to_account_info().try_borrow_mut_lamports()? += refund;

        // Update state
        let bonding_curve = &mut ctx.accounts.bonding_curve;
        bonding_curve.total_supply = bonding_curve.total_supply
            .checked_sub(amount)
            .ok_or(ErrorCode::Overflow)?;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(initial_price: u64, slope: u64)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 8 + 32 + 1, // discriminator + fields
        seeds = [b"bonding_curve".as_ref(), token_mint.key().as_ref()],
        bump
    )]
    pub bonding_curve: Account<'info, BondingCurve>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(initial_price: u64, slope: u64)]
pub struct UpdateParameters<'info> {
    #[account(
        mut,
        seeds = [b"bonding_curve".as_ref(), token_mint.key().as_ref()],
        bump = bonding_curve.bump
    )]
    pub bonding_curve: Account<'info, BondingCurve>,

    #[account(
        constraint = authority.key() == bonding_curve.authority @ ErrorCode::Unauthorized
    )]
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(
        mut, 
        seeds = [b"bonding_curve".as_ref(), token_mint.key().as_ref()], 
        bump = bonding_curve.bump
    )]
    pub bonding_curve: Account<'info, BondingCurve>,

    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub buyer: Signer<'info>,

    #[account(
        mut,
        constraint = buyer_token_account.mint == token_mint.key(),
        constraint = buyer_token_account.owner == buyer.key()
    )]
    pub buyer_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,

    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SellTokens<'info> {
    #[account(
        mut, 
        seeds = [b"bonding_curve".as_ref(), token_mint.key().as_ref()], 
        bump = bonding_curve.bump
    )]
    pub bonding_curve: Account<'info, BondingCurve>,

    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub seller: Signer<'info>,

    #[account(
        mut,
        constraint = seller_token_account.mint == token_mint.key(),
        constraint = seller_token_account.owner == seller.key()
    )]
    pub seller_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub token_mint: Account<'info, Mint>,

    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,

    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[account]
pub struct BondingCurve {
    pub authority: Pubkey,
    pub initial_price: u64,
    pub slope: u64,
    pub total_supply: u64,
    pub token_mint: Pubkey,
    pub bump: u8,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid parameters provided")]
    InvalidParameters,

    #[msg("Overflow occurred during calculation")]
    Overflow,

    #[msg("Unauthorized access")]
    Unauthorized,

    #[msg("Invalid token account")]
    InvalidTokenAccount,

    #[msg("Amount must be greater than 0")]
    InvalidAmount,
} 
