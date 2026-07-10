use anchor_lang::prelude::*;

/// Anchor only allows a single #[error_code] enum per program - all errors
/// from vesting.rs, transfer_hook.rs, and presale.rs are consolidated here.
/// (Previously these were three separate enums, which compiled fine under
/// plain `cargo check` but failed `anchor build` with "Multiple error
/// definitions are not allowed" - Anchor's IDL/error-offset generation
/// assumes exactly one error enum per crate.)
#[error_code]
pub enum MemeCoinError {
    // --- vesting.rs ---
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Cliff period has not been reached yet")]
    CliffNotReached,
    #[msg("Nothing new to release")]
    NothingToRelease,
    #[msg("Pool reserve is empty, cannot compute price")]
    EmptyPoolReserve,
    #[msg("Not enough price history to evaluate volatility")]
    NoPriceHistory,

    // --- transfer_hook.rs ---
    #[msg("Transfer hook invoked outside of an active transfer")]
    NotInTransfer,
    #[msg("DEX trade is missing its required sibling fee-payment instruction to the marketing wallet")]
    MissingFeePayment,

    // --- presale.rs ---
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

    // --- curve.rs ---
    #[msg("Presale has sold out - no more tokens available on the curve")]
    PresaleSoldOut,
    #[msg("Single purchase exceeds the maximum allowed SOL per transaction")]
    PurchaseExceedsMaxPerTx,

    // --- shared across all modules ---
    #[msg("Math overflow")]
    MathOverflow,
}
