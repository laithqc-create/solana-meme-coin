use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::MemeCoinError;
use crate::vesting::VestingState;

pub fn initialize_presale(ctx: Context<InitializePresale>) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
    presale_state.admin = ctx.accounts.admin.key();
    presale_state.total_tokens_sold = 0;
    presale_state.total_sol_raised = 0;
    presale_state.sold_out = false;
    presale_state.is_tge_active = false;
    Ok(())
}

pub fn buy_tokens(ctx: Context<BuyTokens>, amount_sol: u64) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
    let buyer_state = &mut ctx.accounts.buyer_state;

    require!(!presale_state.sold_out, MemeCoinError::PresaleSoldOut);

    // total_tokens_sold is stored in BASE units (matches decimals used
    // everywhere else in the program), but curve.rs works in WHOLE-token
    // units - convert at this boundary only. total_tokens_sold is always
    // an exact multiple of 10^9 since every purchase adds a whole-token
    // amount scaled by 10^9 below, so this division is always exact.
    let cumulative_sold_whole = (presale_state.total_tokens_sold as u128) / 1_000_000_000;

    let purchase = crate::curve::calculate_purchase(amount_sol, cumulative_sold_whole)?;

    let tokens_out_base: u64 = purchase
        .tokens_out_whole
        .checked_mul(1_000_000_000)
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(MemeCoinError::MathOverflow)?;

    let cost_lamports: u64 = u64::try_from(purchase.cost_lamports)
        .map_err(|_| MemeCoinError::MathOverflow)?;

    // Transfer only the true cost under the curve - for a purchase that
    // sells out the presale, this may be LESS than the buyer's requested
    // amount_sol (see curve::calculate_purchase doc comment). We never
    // transfer more than cost_lamports.
    let cpi_context = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        anchor_lang::system_program::Transfer {
            from: ctx.accounts.buyer.to_account_info(),
            to: ctx.accounts.treasury.to_account_info(),
        },
    );
    anchor_lang::system_program::transfer(cpi_context, cost_lamports)?;

    presale_state.total_tokens_sold = presale_state
        .total_tokens_sold
        .checked_add(tokens_out_base)
        .ok_or(MemeCoinError::MathOverflow)?;
    presale_state.total_sol_raised = presale_state
        .total_sol_raised
        .checked_add(cost_lamports)
        .ok_or(MemeCoinError::MathOverflow)?;

    if purchase.tokens_out_whole == (crate::curve::PRESALE_TARGET_TOKENS
        - cumulative_sold_whole)
    {
        // This purchase filled the remaining supply exactly - deterministic
        // sell-out trigger, per the chosen design ("all 300M presale tokens
        // sold out"). Downstream: a separate instruction (not yet built)
        // reads this flag to trigger routing collected funds + the DEX
        // liquidity allocation into a real pool.
        presale_state.sold_out = true;
    }

    // buyer_state may already exist (repeat buyer) via init_if_needed - only
    // total_allocation should accumulate across purchases. claimed_amount
    // and vesting_funded must NOT be reset here: doing so unconditionally
    // on every call would let a buyer who already claimed their 10% TGE
    // allocation buy again and claim a second time, since claim_tge only
    // guards on claimed_amount == 0. Both fields correctly default to
    // their zero values only once, at account creation, via init_if_needed's
    // zero-initialization - never touch them again after that here.
    buyer_state.total_allocation = buyer_state
        .total_allocation
        .checked_add(tokens_out_base)
        .ok_or(MemeCoinError::MathOverflow)?;

    Ok(())
}

pub fn activate_tge(ctx: Context<ActivateTGE>) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
    require!(!presale_state.is_tge_active, MemeCoinError::TGEAlreadyActive);
    presale_state.is_tge_active = true;
    Ok(())
}

pub fn claim_tge(ctx: Context<ClaimTGE>) -> Result<()> {
    let presale_state = &ctx.accounts.presale_state;
    let buyer_state = &mut ctx.accounts.buyer_state;

    require!(presale_state.is_tge_active, MemeCoinError::TGENotActive);
    require!(buyer_state.claimed_amount == 0, MemeCoinError::AlreadyClaimedTGE);

    // 10% TGE unlock
    let claimable = buyer_state.total_allocation / 10;
    buyer_state.claimed_amount += claimable;

    let seeds: &[&[u8]] = &[b"presale_vault", &[ctx.bumps.presale_vault]];
    let signer_seeds: &[&[&[u8]]] = &[seeds];

    let cpi_accounts = TransferChecked {
        from: ctx.accounts.presale_vault.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.buyer_token_account.to_account_info(),
        authority: ctx.accounts.presale_vault.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );

    transfer_checked(cpi_ctx, claimable, ctx.accounts.mint.decimals)?;

    Ok(())
}

/// Second step of a buyer's post-TGE flow. Once a buyer has claimed their
/// 10% TGE unlock, the remaining 90% needs an actual vesting account funded
/// with real tokens before `vesting::release_vesting` has anything to pay
/// out. The client is expected to first call `vesting::initialize_vesting`
/// (beneficiary = this buyer, mint = this mint, total_amount = 90% of their
/// allocation, computed off-chain or read from `buyer_state`) and only then
/// call this instruction, which:
///   1. Verifies the vesting account matches this buyer's expected 90% amount
///      exactly — prevents funding a mismatched or malicious vesting account.
///   2. Moves the 90% out of the presale vault into the vesting PDA's token
///      account, so `release_vesting` has real balance to draw down over
///      time.
pub fn finalize_investor_vesting(ctx: Context<FinalizeInvestorVesting>) -> Result<()> {
    let buyer_state = &mut ctx.accounts.buyer_state;

    require!(buyer_state.claimed_amount > 0, MemeCoinError::TGENotClaimedYet);
    require!(!buyer_state.vesting_funded, MemeCoinError::VestingAlreadyFunded);

    let expected_vesting_amount = buyer_state
        .total_allocation
        .checked_sub(buyer_state.claimed_amount)
        .ok_or(MemeCoinError::MathOverflow)?;

    require_keys_eq!(
        ctx.accounts.vesting_state.beneficiary,
        ctx.accounts.buyer.key(),
        MemeCoinError::VestingBeneficiaryMismatch
    );
    require_keys_eq!(
        ctx.accounts.vesting_state.mint,
        ctx.accounts.mint.key(),
        MemeCoinError::VestingMintMismatch
    );
    require_eq!(
        ctx.accounts.vesting_state.total_amount,
        expected_vesting_amount,
        MemeCoinError::VestingAmountMismatch
    );

    let seeds: &[&[u8]] = &[b"presale_vault", &[ctx.bumps.presale_vault]];
    let signer_seeds: &[&[&[u8]]] = &[seeds];

    let cpi_accounts = TransferChecked {
        from: ctx.accounts.presale_vault.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.vesting_token_account.to_account_info(),
        authority: ctx.accounts.presale_vault.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );

    transfer_checked(cpi_ctx, expected_vesting_amount, ctx.accounts.mint.decimals)?;

    buyer_state.vesting_funded = true;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializePresale<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + 32 + 8 + 8 + 1 + 1,
        seeds = [b"presale_state"],
        bump
    )]
    pub presale_state: Account<'info, PresaleState>,
    #[account(
        init,
        payer = admin,
        seeds = [b"presale_vault"],
        bump,
        token::mint = mint,
        token::authority = presale_vault,
        token::token_program = token_program,
    )]
    pub presale_vault: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ActivateTGE<'info> {
    #[account(mut, has_one = admin @ MemeCoinError::Unauthorized)]
    pub presale_state: Account<'info, PresaleState>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub presale_state: Account<'info, PresaleState>,
    #[account(
        init_if_needed,
        payer = buyer,
        space = 8 + 8 + 8 + 1,
        seeds = [b"buyer", buyer.key().as_ref()],
        bump
    )]
    pub buyer_state: Account<'info, BuyerState>,
    /// CHECK: PDA, seeds enforced below - the ONLY valid destination for
    /// presale SOL. Previously this was an unconstrained AccountInfo, which
    /// meant whoever constructed the buy_tokens transaction could point
    /// investor SOL at ANY account with zero on-chain enforcement - a
    /// direct contradiction of "funds can never be withdrawn by a human."
    /// Anchor's seeds constraint below makes this the only address that
    /// will ever pass account validation, full stop.
    #[account(mut, seeds = [b"presale_treasury"], bump)]
    pub treasury: AccountInfo<'info>,
    #[account(mut)]
    pub buyer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimTGE<'info> {
    pub presale_state: Account<'info, PresaleState>,
    #[account(mut, seeds = [b"buyer", buyer.key().as_ref()], bump)]
    pub buyer_state: Account<'info, BuyerState>,
    #[account(mut, seeds = [b"presale_vault"], bump)]
    pub presale_vault: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub buyer_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub buyer: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct FinalizeInvestorVesting<'info> {
    #[account(mut, seeds = [b"buyer", buyer.key().as_ref()], bump)]
    pub buyer_state: Account<'info, BuyerState>,
    #[account(mut, seeds = [b"presale_vault"], bump)]
    pub presale_vault: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    /// Vesting account, already created via a prior call to
    /// `vesting::initialize_vesting` with beneficiary = buyer.
    pub vesting_state: Account<'info, VestingState>,
    #[account(mut)]
    pub vesting_token_account: InterfaceAccount<'info, TokenAccount>,
    pub buyer: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[account]
pub struct PresaleState {
    pub admin: Pubkey,
    pub total_tokens_sold: u64,
    pub total_sol_raised: u64,
    pub sold_out: bool,
    pub is_tge_active: bool,
}

#[account]
pub struct BuyerState {
    pub total_allocation: u64,
    pub claimed_amount: u64,
    pub vesting_funded: bool,
}

#[cfg(test)]
mod tests {
    #[test]
    fn ninety_percent_split_math() {
        let total_allocation: u64 = 1_000_000_000; // 1000 tokens @ 9dp, illustrative
        let claimed = total_allocation / 10; // 10% TGE
        let expected_vesting = total_allocation - claimed;
        assert_eq!(claimed, 100_000_000);
        assert_eq!(expected_vesting, 900_000_000);
    }
}
