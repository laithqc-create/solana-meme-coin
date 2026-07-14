use anchor_lang::prelude::*;

pub mod errors;
pub mod curve;
pub mod transfer_hook;
pub mod vesting;
pub mod presale;
pub mod liquidity;

use transfer_hook::*;
use vesting::*;
use presale::*;
use liquidity::*;

declare_id!("EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N");

#[program]
pub mod solana_meme_coin {
    use super::*;

    // ========================================
    // Vesting Instructions
    // ========================================

    pub fn initialize_vesting(
        ctx: Context<InitializeVesting>,
        total_amount: u64,
    ) -> Result<()> {
        vesting::initialize_vesting(ctx, total_amount)
    }

    pub fn release_vesting(ctx: Context<ReleaseVesting>) -> Result<()> {
        vesting::release_vesting(ctx)
    }

    pub fn record_price_snapshot(ctx: Context<RecordPriceSnapshot>) -> Result<()> {
        vesting::record_price_snapshot(ctx)
    }

    pub fn check_and_trigger_volatility_delay(ctx: Context<CheckVolatilityDelay>) -> Result<()> {
        vesting::check_and_trigger_volatility_delay(ctx)
    }

    // ========================================
    // Transfer Hook Instructions
    // ========================================

    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        transfer_hook::initialize_extra_account_meta_list(ctx)
    }

    pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
        transfer_hook::transfer_hook(ctx, amount)
    }

    // ========================================
    // Presale Instructions
    // ========================================

    pub fn initialize_presale(ctx: Context<InitializePresale>) -> Result<()> {
        presale::initialize_presale(ctx)
    }

    pub fn activate_tge(ctx: Context<ActivateTGE>) -> Result<()> {
        presale::activate_tge(ctx)
    }

    pub fn buy_tokens(ctx: Context<BuyTokens>, amount_sol: u64) -> Result<()> {
        presale::buy_tokens(ctx, amount_sol)
    }

    pub fn claim_tge(ctx: Context<ClaimTGE>) -> Result<()> {
        presale::claim_tge(ctx)
    }

    pub fn finalize_investor_vesting(ctx: Context<FinalizeInvestorVesting>) -> Result<()> {
        presale::finalize_investor_vesting(ctx)
    }

    // ========================================
    // Liquidity Instructions
    // ========================================

    pub fn seed_liquidity_pool(ctx: Context<SeedLiquidityPool>, open_time: u64) -> Result<()> {
        liquidity::seed_liquidity_pool(ctx, open_time)
    }
}

