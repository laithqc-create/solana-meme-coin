use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{sync_native, Token};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use raydium_cpmm_cpi::{
    cpi, program::RaydiumCpmm, AmmConfig, AUTH_SEED, OBSERVATION_SEED, POOL_LP_MINT_SEED,
    POOL_SEED, POOL_VAULT_SEED,
};

use crate::errors::MemeCoinError;
use crate::presale::PresaleState;

/// Seeds the Raydium CPMM pool once the presale sells out - the final step
/// of Phase A's on-chain flow. Wraps ALL collected presale SOL into WSOL,
/// then deposits it alongside the (already off-chain-funded) 30%
/// DEX-liquidity token allocation into a brand-new Raydium CPMM pool via
/// CPI, in whichever token_0/token_1 order Raydium's program requires
/// (byte-comparison of the two mint pubkeys - NOT knowable until runtime,
/// so every Raydium-side account below is verified imperatively in this
/// function body against the real PDA-derivation formula, rather than via
/// Anchor's declarative `seeds = [...]` constraints, which can't easily
/// express "derive using whichever of these two mints turns out smaller."
/// This mirrors Raydium's own official CPI example, which also leaves
/// these accounts as plain UncheckedAccounts on the caller's side and lets
/// Raydium's own inner instruction do the authoritative validation - our
/// manual checks here are an extra layer, not a replacement for that.
pub fn seed_liquidity_pool(ctx: Context<SeedLiquidityPool>, open_time: u64) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
    require!(presale_state.sold_out, MemeCoinError::PresaleNotSoldOut);
    require!(
        !presale_state.liquidity_seeded,
        MemeCoinError::LiquidityAlreadySeeded
    );

    // --- Step 1: wrap the entire collected presale SOL balance into WSOL ---
    // presale_treasury is a plain system-owned PDA (never `init`-ed as a
    // program account - see presale.rs), so its full lamport balance is
    // free to move. We drain it completely: nothing downstream needs this
    // PDA to remain rent-exempt or to exist at all afterward.
    let treasury_lamports = ctx.accounts.treasury.lamports();

    let treasury_seeds: &[&[u8]] = &[b"presale_treasury", &[ctx.bumps.treasury]];
    let treasury_signer: &[&[&[u8]]] = &[treasury_seeds];
    system_program::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.treasury.to_account_info(),
                to: ctx.accounts.wsol_vault.to_account_info(),
            },
            treasury_signer,
        ),
        treasury_lamports,
    )?;

    sync_native(CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        anchor_spl::token::SyncNative {
            account: ctx.accounts.wsol_vault.to_account_info(),
        },
    ))?;
    ctx.accounts.wsol_vault.reload()?;
    let wsol_amount = ctx.accounts.wsol_vault.amount;

    let our_mint_amount = ctx.accounts.dex_liquidity_vault.amount;

    // --- Step 2: determine Raydium's required token_0/token_1 ordering ---
    // Raydium's Initialize enforces token_0_mint.key() < token_1_mint.key()
    // (see the real Initialize account constraints in raydium-cpmm-cpi) -
    // this is a runtime fact about two specific pubkeys, not something
    // knowable when writing this code, so both branches are handled below.
    let our_mint_key = ctx.accounts.mint.key();
    let wsol_mint_key = ctx.accounts.wsol_mint.key();
    require!(our_mint_key != wsol_mint_key, MemeCoinError::MathOverflow); // sanity: can't ever actually happen, mint is never WSOL

    let (token_0_mint_key, token_1_mint_key) = if our_mint_key < wsol_mint_key {
        (our_mint_key, wsol_mint_key)
    } else {
        (wsol_mint_key, our_mint_key)
    };

    // --- Step 3: verify every Raydium-owned PDA against the real formula ---
    let cp_swap_program_id = ctx.accounts.cp_swap_program.key();

    let (expected_authority, _) =
        Pubkey::find_program_address(&[AUTH_SEED.as_bytes()], &cp_swap_program_id);
    require_keys_eq!(
        ctx.accounts.authority.key(),
        expected_authority,
        MemeCoinError::RaydiumAuthorityMismatch
    );

    let (expected_pool_state, _) = Pubkey::find_program_address(
        &[
            POOL_SEED.as_bytes(),
            ctx.accounts.amm_config.key().as_ref(),
            token_0_mint_key.as_ref(),
            token_1_mint_key.as_ref(),
        ],
        &cp_swap_program_id,
    );
    require_keys_eq!(
        ctx.accounts.pool_state.key(),
        expected_pool_state,
        MemeCoinError::PoolStateMismatch
    );

    let (expected_lp_mint, _) = Pubkey::find_program_address(
        &[
            POOL_LP_MINT_SEED.as_bytes(),
            expected_pool_state.as_ref(),
        ],
        &cp_swap_program_id,
    );
    require_keys_eq!(
        ctx.accounts.lp_mint.key(),
        expected_lp_mint,
        MemeCoinError::LpMintMismatch
    );

    let (expected_token_0_vault, _) = Pubkey::find_program_address(
        &[
            POOL_VAULT_SEED.as_bytes(),
            expected_pool_state.as_ref(),
            token_0_mint_key.as_ref(),
        ],
        &cp_swap_program_id,
    );
    require_keys_eq!(
        ctx.accounts.token_0_vault.key(),
        expected_token_0_vault,
        MemeCoinError::TokenVaultMismatch
    );

    let (expected_token_1_vault, _) = Pubkey::find_program_address(
        &[
            POOL_VAULT_SEED.as_bytes(),
            expected_pool_state.as_ref(),
            token_1_mint_key.as_ref(),
        ],
        &cp_swap_program_id,
    );
    require_keys_eq!(
        ctx.accounts.token_1_vault.key(),
        expected_token_1_vault,
        MemeCoinError::TokenVaultMismatch
    );

    let (expected_observation_state, _) = Pubkey::find_program_address(
        &[OBSERVATION_SEED.as_bytes(), expected_pool_state.as_ref()],
        &cp_swap_program_id,
    );
    require_keys_eq!(
        ctx.accounts.observation_state.key(),
        expected_observation_state,
        MemeCoinError::ObservationStateMismatch
    );

    // --- Step 4: route our two funding accounts into the correct CPI slots ---
    let (
        cpi_token_0_mint,
        cpi_token_1_mint,
        cpi_creator_token_0,
        cpi_creator_token_1,
        cpi_token_0_program,
        cpi_token_1_program,
        init_amount_0,
        init_amount_1,
    ) = if our_mint_key < wsol_mint_key {
        (
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.wsol_mint.to_account_info(),
            ctx.accounts.dex_liquidity_vault.to_account_info(),
            ctx.accounts.wsol_vault.to_account_info(),
            ctx.accounts.token_program_2022.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            our_mint_amount,
            wsol_amount,
        )
    } else {
        (
            ctx.accounts.wsol_mint.to_account_info(),
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.wsol_vault.to_account_info(),
            ctx.accounts.dex_liquidity_vault.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.token_program_2022.to_account_info(),
            wsol_amount,
            our_mint_amount,
        )
    };

    // --- Step 5: CPI into Raydium CPMM's initialize instruction ---
    let authority_seeds: &[&[u8]] = &[
        b"pool_creator_authority",
        &[ctx.bumps.pool_creator_authority],
    ];
    let authority_signer: &[&[&[u8]]] = &[authority_seeds];

    let cpi_accounts = cpi::accounts::Initialize {
        creator: ctx.accounts.pool_creator_authority.to_account_info(),
        amm_config: ctx.accounts.amm_config.to_account_info(),
        authority: ctx.accounts.authority.to_account_info(),
        pool_state: ctx.accounts.pool_state.to_account_info(),
        token_0_mint: cpi_token_0_mint,
        token_1_mint: cpi_token_1_mint,
        lp_mint: ctx.accounts.lp_mint.to_account_info(),
        creator_token_0: cpi_creator_token_0,
        creator_token_1: cpi_creator_token_1,
        creator_lp_token: ctx.accounts.creator_lp_token.to_account_info(),
        token_0_vault: ctx.accounts.token_0_vault.to_account_info(),
        token_1_vault: ctx.accounts.token_1_vault.to_account_info(),
        create_pool_fee: ctx.accounts.create_pool_fee.to_account_info(),
        observation_state: ctx.accounts.observation_state.to_account_info(),
        token_program: ctx.accounts.token_program.to_account_info(),
        token_0_program: cpi_token_0_program,
        token_1_program: cpi_token_1_program,
        associated_token_program: ctx.accounts.associated_token_program.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        rent: ctx.accounts.rent.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.cp_swap_program.to_account_info(),
        cpi_accounts,
        authority_signer,
    );

    cpi::initialize(cpi_ctx, init_amount_0, init_amount_1, open_time)?;

    presale_state.liquidity_seeded = true;

    Ok(())
}

#[derive(Accounts)]
pub struct SeedLiquidityPool<'info> {
    #[account(mut, has_one = admin @ MemeCoinError::Unauthorized)]
    pub presale_state: Account<'info, PresaleState>,
    /// CHECK: PDA, seeds enforced - drained completely in step 1.
    #[account(mut, seeds = [b"presale_treasury"], bump)]
    pub treasury: AccountInfo<'info>,
    #[account(mut, seeds = [b"dex_liquidity_vault"], bump)]
    pub dex_liquidity_vault: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: empty PDA, seeds enforced - see InitializePresale in presale.rs
    /// for why this single PDA has to double as authority over both funding
    /// vaults AND the Raydium CPI "creator" signer.
    #[account(seeds = [b"pool_creator_authority"], bump)]
    pub pool_creator_authority: AccountInfo<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    /// Solana's native-SOL wrapper mint - always this fixed address,
    /// always the legacy (non-2022) token program.
    #[account(address = anchor_spl::token::spl_token::native_mint::ID)]
    pub wsol_mint: InterfaceAccount<'info, Mint>,
    /// WSOL holding account for the wrapped presale SOL. Created here
    /// (init_if_needed so a retried/resumed flow doesn't fail on a second
    /// attempt) as an ATA owned by pool_creator_authority.
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = wsol_mint,
        associated_token::authority = pool_creator_authority,
    )]
    pub wsol_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub admin: Signer<'info>,

    // --- Raydium CPMM accounts - see module doc comment for why these are
    // UncheckedAccount and verified imperatively above rather than via
    // declarative seeds constraints ---
    /// Raydium's fee-tier config for this pool - an already-existing
    /// account, address varies by cluster (devnet vs mainnet) and is
    /// supplied by the caller rather than hardcoded here, since Raydium's
    /// devnet deployment details are less stable than mainnet's.
    pub amm_config: Box<Account<'info, AmmConfig>>,
    /// CHECK: verified against the real AUTH_SEED PDA formula above.
    pub authority: UncheckedAccount<'info>,
    /// CHECK: verified against the real POOL_SEED PDA formula above -
    /// created by Raydium's own CPI-invoked instruction, not by us.
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,
    /// CHECK: verified against the real POOL_LP_MINT_SEED PDA formula above.
    #[account(mut)]
    pub lp_mint: UncheckedAccount<'info>,
    /// CHECK: created by Raydium's own CPI-invoked instruction as an ATA
    /// owned by pool_creator_authority - does not exist yet when this
    /// instruction's accounts are validated, so must stay UncheckedAccount
    /// on our side (an `init` here would race Raydium's own `init` for the
    /// same account).
    #[account(mut)]
    pub creator_lp_token: UncheckedAccount<'info>,
    /// CHECK: verified against the real POOL_VAULT_SEED PDA formula above.
    #[account(mut)]
    pub token_0_vault: UncheckedAccount<'info>,
    /// CHECK: verified against the real POOL_VAULT_SEED PDA formula above.
    #[account(mut)]
    pub token_1_vault: UncheckedAccount<'info>,
    /// Raydium's fixed pool-creation fee-receiver token account - address
    /// varies by cluster, supplied by the caller (see amm_config doc above).
    #[account(mut)]
    pub create_pool_fee: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: verified against the real OBSERVATION_SEED PDA formula above.
    #[account(mut)]
    pub observation_state: UncheckedAccount<'info>,

    /// Legacy SPL Token program - Raydium's LP mint is always legacy SPL,
    /// and this doubles as the WSOL vault's token program (WSOL is always
    /// legacy SPL too, never Token-2022).
    pub token_program: Program<'info, Token>,
    /// Our mint's real token program (Token-2022) - separate from
    /// `token_program` above because our mint and WSOL use different SPL
    /// program generations; Raydium's Initialize takes both independently
    /// as token_0_program/token_1_program (Interface<TokenInterface>),
    /// routed to the correct slot in step 4 above.
    pub token_program_2022: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
    pub cp_swap_program: Program<'info, RaydiumCpmm>,
}
