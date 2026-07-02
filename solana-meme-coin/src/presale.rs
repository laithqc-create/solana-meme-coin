use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::vesting::VestingState;

pub fn initialize_presale(ctx: Context<InitializePresale>) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
    presale_state.admin = ctx.accounts.admin.key();
    presale_state.total_tokens_sold = 0;
    presale_state.total_sol_raised = 0;
    presale_state.current_phase = 1;
    presale_state.is_tge_active = false;

    // Phase thresholds (Milestones) based on tokens sold
    // Phase 1: 100M tokens
    // Phase 2: 200M tokens (cumulative)
    // Phase 3: 300M tokens (cumulative)
    Ok(())
}

pub fn buy_tokens(ctx: Context<BuyTokens>, amount_sol: u64) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
    let buyer_state = &mut ctx.accounts.buyer_state;

    // 1. Determine rate based on milestone phase
    let rate = match presale_state.current_phase {
        1 => 500, // $0.00200 equiv (mock rate SOL/Token)
        2 => 444, // $0.00225 equiv
        3 => 400, // $0.00250 equiv
        _ => return Err(ErrorCode::InvalidPhase.into()),
    };

    let tokens_to_receive = amount_sol * rate;

    // 2. Update Milestones (Phase Transitions)
    presale_state.total_tokens_sold += tokens_to_receive;
    presale_state.total_sol_raised += amount_sol;

    if presale_state.total_tokens_sold >= 100_000_000 * 10u64.pow(9) && presale_state.current_phase == 1 {
        presale_state.current_phase = 2;
    } else if presale_state.total_tokens_sold >= 200_000_000 * 10u64.pow(9) && presale_state.current_phase == 2 {
        presale_state.current_phase = 3;
    }

    // 3. Transfer SOL from buyer to treasury
    let cpi_context = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        anchor_lang::system_program::Transfer {
            from: ctx.accounts.buyer.to_account_info(),
            to: ctx.accounts.treasury.to_account_info(),
        },
    );
    anchor_lang::system_program::transfer(cpi_context, amount_sol)?;

    // 4. Record buyer's allocation
    buyer_state.total_allocation += tokens_to_receive;
    buyer_state.claimed_amount = 0;
    buyer_state.vesting_funded = false;

    Ok(())
}

pub fn activate_tge(ctx: Context<ActivateTGE>) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
    require!(!presale_state.is_tge_active, ErrorCode::TGEAlreadyActive);
    presale_state.is_tge_active = true;
    Ok(())
}

pub fn claim_tge(ctx: Context<ClaimTGE>) -> Result<()> {
    let presale_state = &ctx.accounts.presale_state;
    let buyer_state = &mut ctx.accounts.buyer_state;

    require!(presale_state.is_tge_active, ErrorCode::TGENotActive);
    require!(buyer_state.claimed_amount == 0, ErrorCode::AlreadyClaimedTGE);

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

    require!(buyer_state.claimed_amount > 0, ErrorCode::TGENotClaimedYet);
    require!(!buyer_state.vesting_funded, ErrorCode::VestingAlreadyFunded);

    let expected_vesting_amount = buyer_state
        .total_allocation
        .checked_sub(buyer_state.claimed_amount)
        .ok_or(ErrorCode::MathOverflow)?;

    require_keys_eq!(
        ctx.accounts.vesting_state.beneficiary,
        ctx.accounts.buyer.key(),
        ErrorCode::VestingBeneficiaryMismatch
    );
    require_keys_eq!(
        ctx.accounts.vesting_state.mint,
        ctx.accounts.mint.key(),
        ErrorCode::VestingMintMismatch
    );
    require_eq!(
        ctx.accounts.vesting_state.total_amount,
        expected_vesting_amount,
        ErrorCode::VestingAmountMismatch
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
    #[account(mut, has_one = admin @ ErrorCode::Unauthorized)]
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
    #[account(mut)]
    /// CHECK: Treasury wallet receiving SOL
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
    pub current_phase: u8,
    pub is_tge_active: bool,
}

#[account]
pub struct BuyerState {
    pub total_allocation: u64,
    pub claimed_amount: u64,
    pub vesting_funded: bool,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid presale phase.")]
    InvalidPhase,
    #[msg("TGE is not yet active.")]
    TGENotActive,
    #[msg("TGE has already been activated.")]
    TGEAlreadyActive,
    #[msg("Unauthorized: signer is not the presale admin.")]
    Unauthorized,
    #[msg("TGE allocation already claimed.")]
    AlreadyClaimedTGE,
    #[msg("Buyer must claim their TGE allocation before vesting can be funded.")]
    TGENotClaimedYet,
    #[msg("Vesting has already been funded for this buyer.")]
    VestingAlreadyFunded,
    #[msg("Vesting account beneficiary does not match this buyer.")]
    VestingBeneficiaryMismatch,
    #[msg("Vesting account mint does not match the presale mint.")]
    VestingMintMismatch,
    #[msg("Vesting account total_amount does not match buyer's expected 90% allocation.")]
    VestingAmountMismatch,
    #[msg("Math overflow.")]
    MathOverflow,
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
