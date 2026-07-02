use anchor_lang::prelude::*;
use anchor_spl::token_interface::{TokenAccount, TokenInterface, TransferChecked, transfer_checked};
use anchor_spl::token_interface::Mint;

/// ---- Time constants (all schedules use 30-day "months" for consistency
/// with the presale doc, which specifies both the cliff and the monthly
/// unlock rate in 30-day units) ----
pub const CLIFF_SECONDS: i64 = 30 * 24 * 60 * 60; // 30 days
pub const MONTH_SECONDS: i64 = 30 * 24 * 60 * 60; // 30 days
pub const VOLATILITY_WINDOW_SECONDS: i64 = 48 * 60 * 60; // 48 hours
pub const VOLATILITY_DELAY_SECONDS: i64 = 7 * 24 * 60 * 60; // 7 days
pub const MONTHLY_UNLOCK_BPS: u64 = 800; // 8.00% in basis points
pub const DROP_TRIGGER_BPS: u64 = 3000; // 30.00% drop triggers delay
pub const BPS_DENOMINATOR: u64 = 10_000;
pub const PRICE_HISTORY_LEN: usize = 48; // one snapshot per hour, 48h window
pub const PRICE_PRECISION: u128 = 1_000_000_000; // fixed-point scale for price ratio

// ============================================================
// Initialize
// ============================================================

pub fn initialize_vesting(
    ctx: Context<InitializeVesting>,
    total_amount: u64,
) -> Result<()> {
    let vesting_state = &mut ctx.accounts.vesting_state;
    let clock = Clock::get()?;

    require!(total_amount > 0, VestingError::ZeroAmount);

    vesting_state.beneficiary = ctx.accounts.beneficiary.key();
    vesting_state.mint = ctx.accounts.mint.key();
    vesting_state.total_amount = total_amount;
    vesting_state.released_amount = 0;
    vesting_state.start_time = clock.unix_timestamp;
    vesting_state.cliff_end_time = clock.unix_timestamp + CLIFF_SECONDS;
    vesting_state.delay_active = false;
    vesting_state.delay_expires_at = 0;
    vesting_state.delayed_month_index = 0;
    vesting_state.price_history = [PriceSnapshot { timestamp: 0, price: 0 }; PRICE_HISTORY_LEN];
    vesting_state.price_history_write_idx = 0;
    vesting_state.price_history_count = 0;
    vesting_state.bump = ctx.bumps.vesting_state;

    Ok(())
}

// ============================================================
// Price snapshot crank — permissionless, meant to be called by a
// keeper/cron roughly once per hour. Reads the two AMM vault token
// accounts directly (their live balances) rather than parsing any
// pool program's internal layout, so it works against Raydium CPMM
// or Meteora Dynamic AMM vaults without depending on their SDKs.
// ============================================================

pub fn record_price_snapshot(ctx: Context<RecordPriceSnapshot>) -> Result<()> {
    let clock = Clock::get()?;
    let base_reserve = ctx.accounts.pool_base_vault.amount;
    let quote_reserve = ctx.accounts.pool_quote_vault.amount;

    require!(base_reserve > 0, VestingError::EmptyPoolReserve);

    // price = quote per base, fixed-point scaled
    let price: u128 = (quote_reserve as u128)
        .checked_mul(PRICE_PRECISION)
        .ok_or(VestingError::MathOverflow)?
        .checked_div(base_reserve as u128)
        .ok_or(VestingError::MathOverflow)?;

    require!(price <= u64::MAX as u128, VestingError::MathOverflow);

    let vesting_state = &mut ctx.accounts.vesting_state;
    let idx = vesting_state.price_history_write_idx as usize;
    vesting_state.price_history[idx] = PriceSnapshot {
        timestamp: clock.unix_timestamp,
        price: price as u64,
    };
    vesting_state.price_history_write_idx =
        ((idx + 1) % PRICE_HISTORY_LEN) as u8;
    if (vesting_state.price_history_count as usize) < PRICE_HISTORY_LEN {
        vesting_state.price_history_count += 1;
    }

    Ok(())
}

// ============================================================
// Volatility check — permissionless. Compares the freshest snapshot
// against the oldest snapshot still inside the 48h rolling window.
// If price fell by >= 30%, delays only the tranche that is currently
// maturing, by up to 7 days. Does NOT shift the timestamp grid for
// any future month — future unlocks are still computed directly off
// `start_time + CLIFF_SECONDS + n * MONTH_SECONDS`.
// ============================================================

pub fn check_and_trigger_volatility_delay(ctx: Context<CheckVolatilityDelay>) -> Result<()> {
    let clock = Clock::get()?;
    let vesting_state = &mut ctx.accounts.vesting_state;

    require!(vesting_state.price_history_count > 0, VestingError::NoPriceHistory);

    // Freshest snapshot = the most recently written slot.
    let latest_idx = if vesting_state.price_history_write_idx == 0 {
        PRICE_HISTORY_LEN - 1
    } else {
        (vesting_state.price_history_write_idx - 1) as usize
    };
    let latest = vesting_state.price_history[latest_idx];
    require!(latest.timestamp > 0, VestingError::NoPriceHistory);

    // Oldest snapshot that still falls within the last 48h.
    let window_start = clock.unix_timestamp - VOLATILITY_WINDOW_SECONDS;
    let mut reference: Option<PriceSnapshot> = None;
    let count = vesting_state.price_history_count as usize;
    for i in 0..count {
        let snap = vesting_state.price_history[i];
        if snap.timestamp >= window_start
            && (reference.is_none() || snap.timestamp < reference.unwrap().timestamp)
        {
            reference = Some(snap);
        }
    }

    let reference = match reference {
        Some(r) => r,
        // Not enough history yet to evaluate a full 48h window — no-op.
        None => return Ok(()),
    };

    if reference.price == 0 {
        return Ok(());
    }

    // Only evaluate a drop, never a rise.
    if latest.price >= reference.price {
        return Ok(());
    }

    let drop_bps = ((reference.price - latest.price) as u128)
        .checked_mul(BPS_DENOMINATOR as u128)
        .ok_or(VestingError::MathOverflow)?
        .checked_div(reference.price as u128)
        .ok_or(VestingError::MathOverflow)?;

    if drop_bps >= DROP_TRIGGER_BPS as u128 {
        let current_month_idx = current_maturing_month_index(vesting_state, clock.unix_timestamp);

        // Only the first trigger for a given maturing month applies —
        // don't let repeated cranks re-extend the same delay.
        if !(vesting_state.delay_active && vesting_state.delayed_month_index == current_month_idx) {
            vesting_state.delay_active = true;
            vesting_state.delayed_month_index = current_month_idx;
            vesting_state.delay_expires_at = clock.unix_timestamp + VOLATILITY_DELAY_SECONDS;
            msg!(
                "Volatility delay triggered: {}bps drop over 48h, month index {} delayed until {}",
                drop_bps,
                current_month_idx,
                vesting_state.delay_expires_at
            );
        }
    }

    Ok(())
}

/// Which monthly tranche index (0-based, first tranche unlocks at
/// cliff_end_time) is the one currently in the process of maturing.
fn current_maturing_month_index(vesting_state: &VestingState, now: i64) -> u32 {
    if now < vesting_state.cliff_end_time {
        return 0;
    }
    let elapsed = now - vesting_state.cliff_end_time;
    (elapsed / MONTH_SECONDS) as u32
}

// ============================================================
// Release — computes vested amount from the fixed calendar grid,
// withholds the single most-recently-matured tranche if a volatility
// delay is currently active and hasn't expired yet, and transfers
// the newly-vested delta to the beneficiary.
// ============================================================

pub fn release_vesting(ctx: Context<ReleaseVesting>) -> Result<()> {
    let clock = Clock::get()?;
    let vesting_state = &mut ctx.accounts.vesting_state;

    require!(clock.unix_timestamp >= vesting_state.cliff_end_time, VestingError::CliffNotReached);

    let elapsed = clock.unix_timestamp - vesting_state.cliff_end_time;
    let mut months_matured: u64 = (elapsed / MONTH_SECONDS) as u64 + 1; // +1: cliff_end_time itself unlocks tranche 0

    // Cap at fully-vested (100% / 8% = 12.5 -> 13 tranches to cover the remainder)
    let max_tranches = (BPS_DENOMINATOR + MONTHLY_UNLOCK_BPS - 1) / MONTHLY_UNLOCK_BPS; // ceil(10000/800) = 13
    if months_matured > max_tranches {
        months_matured = max_tranches;
    }

    // If a volatility delay is active for the most recent tranche and
    // it hasn't expired yet, withhold that one tranche only.
    let current_month_idx = current_maturing_month_index(vesting_state, clock.unix_timestamp);
    if vesting_state.delay_active
        && vesting_state.delayed_month_index == current_month_idx
        && clock.unix_timestamp < vesting_state.delay_expires_at
        && months_matured > 0
    {
        months_matured -= 1;
    }

    let vested_bps = core::cmp::min(
        months_matured.saturating_mul(MONTHLY_UNLOCK_BPS),
        BPS_DENOMINATOR,
    );

    let total_vested = (vesting_state.total_amount as u128)
        .checked_mul(vested_bps as u128)
        .ok_or(VestingError::MathOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(VestingError::MathOverflow)? as u64;

    require!(total_vested > vesting_state.released_amount, VestingError::NothingToRelease);

    let release_amount = total_vested - vesting_state.released_amount;

    let mint_key = vesting_state.mint;
    let beneficiary_key = vesting_state.beneficiary;
    let seeds: &[&[u8]] = &[
        b"vesting",
        beneficiary_key.as_ref(),
        mint_key.as_ref(),
        &[vesting_state.bump],
    ];
    let signer_seeds: &[&[&[u8]]] = &[seeds];

    let cpi_accounts = TransferChecked {
        from: ctx.accounts.vesting_token_account.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.beneficiary_token_account.to_account_info(),
        authority: ctx.accounts.vesting_state.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    transfer_checked(cpi_ctx, release_amount, ctx.accounts.mint.decimals)?;

    ctx.accounts.vesting_state.released_amount = total_vested;

    Ok(())
}

// ============================================================
// Accounts
// ============================================================

#[derive(Clone, Copy, AnchorSerialize, AnchorDeserialize, Default)]
pub struct PriceSnapshot {
    pub timestamp: i64,
    pub price: u64,
}

#[derive(Accounts)]
pub struct InitializeVesting<'info> {
    #[account(
        init,
        payer = payer,
        space = 8  // discriminator
            + 32   // beneficiary
            + 32   // mint
            + 8    // total_amount
            + 8    // released_amount
            + 8    // start_time
            + 8    // cliff_end_time
            + 1    // delay_active
            + 8    // delay_expires_at
            + 4    // delayed_month_index
            + (16 * PRICE_HISTORY_LEN) // price_history
            + 1    // price_history_write_idx
            + 2    // price_history_count
            + 1,   // bump
        seeds = [b"vesting", beneficiary.key().as_ref(), mint.key().as_ref()],
        bump
    )]
    pub vesting_state: Account<'info, VestingState>,
    /// CHECK: recipient of the eventual vested tokens; doesn't need to sign init
    pub beneficiary: UncheckedAccount<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RecordPriceSnapshot<'info> {
    #[account(mut)]
    pub vesting_state: Account<'info, VestingState>,
    /// Base-token vault of the AMM pool (this project's token side).
    /// CONSTRAINT: caller must pass the correct, known pool vault —
    /// this instruction does not itself verify the vault belongs to
    /// the canonical pool. Wire a hardcoded pool-vault pubkey check
    /// here before mainnet deploy.
    pub pool_base_vault: InterfaceAccount<'info, TokenAccount>,
    /// Quote-token vault (USDC/SOL side) of the same pool.
    pub pool_quote_vault: InterfaceAccount<'info, TokenAccount>,
}

#[derive(Accounts)]
pub struct CheckVolatilityDelay<'info> {
    #[account(mut)]
    pub vesting_state: Account<'info, VestingState>,
}

#[derive(Accounts)]
pub struct ReleaseVesting<'info> {
    #[account(
        mut,
        seeds = [b"vesting", vesting_state.beneficiary.as_ref(), vesting_state.mint.as_ref()],
        bump = vesting_state.bump,
        has_one = mint,
    )]
    pub vesting_state: Account<'info, VestingState>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub vesting_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub beneficiary_token_account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[account]
pub struct VestingState {
    pub beneficiary: Pubkey,
    pub mint: Pubkey,
    pub total_amount: u64,
    pub released_amount: u64,
    pub start_time: i64,
    pub cliff_end_time: i64,
    pub delay_active: bool,
    pub delay_expires_at: i64,
    pub delayed_month_index: u32,
    pub price_history: [PriceSnapshot; PRICE_HISTORY_LEN],
    pub price_history_write_idx: u8,
    pub price_history_count: u16,
    pub bump: u8,
}

#[error_code]
pub enum VestingError {
    #[msg("Total vesting amount must be greater than zero")]
    ZeroAmount,
    #[msg("Cliff period has not been reached yet")]
    CliffNotReached,
    #[msg("Nothing new to release")]
    NothingToRelease,
    #[msg("Pool reserve is empty, cannot compute price")]
    EmptyPoolReserve,
    #[msg("Not enough price history to evaluate volatility")]
    NoPriceHistory,
    #[msg("Math overflow")]
    MathOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_state(now: i64) -> VestingState {
        VestingState {
            beneficiary: Pubkey::default(),
            mint: Pubkey::default(),
            total_amount: 200_000_000_000_000, // 200M tokens @ 9 decimals
            released_amount: 0,
            start_time: now,
            cliff_end_time: now + CLIFF_SECONDS,
            delay_active: false,
            delay_expires_at: 0,
            delayed_month_index: 0,
            price_history: [PriceSnapshot { timestamp: 0, price: 0 }; PRICE_HISTORY_LEN],
            price_history_write_idx: 0,
            price_history_count: 0,
            bump: 255,
        }
    }

    #[test]
    fn cliff_math() {
        let start_time: i64 = 1_600_000_000;
        assert_eq!(start_time + CLIFF_SECONDS, 1_602_592_000);
    }

    #[test]
    fn first_tranche_matures_exactly_at_cliff() {
        let now: i64 = 1_600_000_000;
        let state = base_state(now);
        let idx = current_maturing_month_index(&state, state.cliff_end_time);
        assert_eq!(idx, 0);
    }

    #[test]
    fn month_index_advances_on_schedule() {
        let now: i64 = 1_600_000_000;
        let state = base_state(now);
        let idx = current_maturing_month_index(&state, state.cliff_end_time + MONTH_SECONDS * 3);
        assert_eq!(idx, 3);
    }

    #[test]
    fn eight_percent_per_month_bps_math() {
        // 5 months matured -> 40% vested
        let vested_bps = core::cmp::min(5u64 * MONTHLY_UNLOCK_BPS, BPS_DENOMINATOR);
        assert_eq!(vested_bps, 4000);
    }

    #[test]
    fn fully_vests_by_thirteenth_tranche() {
        let max_tranches = (BPS_DENOMINATOR + MONTHLY_UNLOCK_BPS - 1) / MONTHLY_UNLOCK_BPS;
        assert_eq!(max_tranches, 13);
        let vested_bps = core::cmp::min(max_tranches * MONTHLY_UNLOCK_BPS, BPS_DENOMINATOR);
        assert_eq!(vested_bps, BPS_DENOMINATOR);
    }

    #[test]
    fn thirty_percent_drop_detection() {
        let reference_price: u128 = 1_000_000_000;
        let latest_price: u128 = 690_000_000; // 31% drop
        let drop_bps = (reference_price - latest_price) * 10_000 / reference_price;
        assert!(drop_bps >= DROP_TRIGGER_BPS as u128);
    }

    #[test]
    fn twenty_nine_percent_drop_does_not_trigger() {
        let reference_price: u128 = 1_000_000_000;
        let latest_price: u128 = 710_000_000; // 29% drop
        let drop_bps = (reference_price - latest_price) * 10_000 / reference_price;
        assert!(drop_bps < DROP_TRIGGER_BPS as u128);
    }

    #[test]
    fn delay_does_not_shift_future_month_grid() {
        let now: i64 = 1_600_000_000;
        let mut state = base_state(now);
        state.delay_active = true;
        state.delayed_month_index = 2;
        state.delay_expires_at = state.cliff_end_time + MONTH_SECONDS * 2 + VOLATILITY_DELAY_SECONDS;
        let idx_before = current_maturing_month_index(&state, state.cliff_end_time + MONTH_SECONDS * 4);
        state.delay_active = false;
        let idx_after = current_maturing_month_index(&state, state.cliff_end_time + MONTH_SECONDS * 4);
        assert_eq!(idx_before, idx_after);
        assert_eq!(idx_before, 4);
    }
}
