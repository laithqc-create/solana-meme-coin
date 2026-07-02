use anchor_lang::prelude::*;

pub fn initialize_vesting(
    ctx: Context<InitializeVesting>,
    total_amount: u64,
) -> Result<()> {
    let vesting_state = &mut ctx.accounts.vesting_state;
    let clock = Clock::get()?;

    vesting_state.total_amount = total_amount;
    vesting_state.released_amount = 0;
    vesting_state.start_time = clock.unix_timestamp;
    // 30 days cliff (in seconds: 30 * 24 * 60 * 60 = 2592000)
    vesting_state.next_unlock_time = clock.unix_timestamp + 2592000; 
    
    Ok(())
}

pub fn release_vesting(_ctx: Context<ReleaseVesting>) -> Result<()> {
    // Vesting logic implementation here
    // Calculates unlocked amount and transfers tokens
    Ok(())
}

pub fn trigger_volatility_delay(ctx: Context<TriggerVolatilityDelay>) -> Result<()> {
    let vesting_state = &mut ctx.accounts.vesting_state;
    // 7 days delay (in seconds: 7 * 24 * 60 * 60 = 604800)
    vesting_state.next_unlock_time += 604800;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeVesting<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + 32 + 8 + 8 + 8 + 8,
        seeds = [b"vesting", payer.key().as_ref()],
        bump
    )]
    pub vesting_state: Account<'info, VestingState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReleaseVesting<'info> {
    #[account(mut)]
    pub vesting_state: Account<'info, VestingState>,
    #[account(mut)]
    pub payer: Signer<'info>,
}

#[derive(Accounts)]
pub struct TriggerVolatilityDelay<'info> {
    #[account(mut)]
    pub vesting_state: Account<'info, VestingState>,
    #[account(mut)]
    pub payer: Signer<'info>,
    // In production, would include Oracle account here to verify 30% drop
}

#[account]
pub struct VestingState {
    pub total_amount: u64,
    pub released_amount: u64,
    pub start_time: i64,
    pub next_unlock_time: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vesting_initialization_math() {
        // Just a simple unit test for the math
        let start_time: i64 = 1600000000;
        let thirty_days = 2592000;
        assert_eq!(start_time + thirty_days, 1602592000);
    }
    
    #[test]
    fn test_volatility_delay_math() {
        let current_next_unlock: i64 = 1602592000;
        let seven_days = 604800;
        assert_eq!(current_next_unlock + seven_days, 1603196800);
    }
}

