use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

pub fn initialize_presale(ctx: Context<InitializePresale>) -> Result<()> {
    let presale_state = &mut ctx.accounts.presale_state;
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

    let seeds = &[b"presale_vault", &[ctx.bumps.presale_vault]];
    let signer = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.presale_vault.to_account_info(),
        to: ctx.accounts.buyer_token_account.to_account_info(),
        authority: ctx.accounts.presale_vault.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

    token::transfer(cpi_ctx, claimable)?;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializePresale<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + 8 + 8 + 1 + 1,
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
    )]
    pub presale_vault: Account<'info, TokenAccount>,
    pub mint: Account<'info, Mint>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub presale_state: Account<'info, PresaleState>,
    #[account(
        init_if_needed,
        payer = buyer,
        space = 8 + 8 + 8,
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
#[instruction()]
pub struct ClaimTGE<'info> {
    pub presale_state: Account<'info, PresaleState>,
    #[account(mut, seeds = [b"buyer", buyer.key().as_ref()], bump)]
    pub buyer_state: Account<'info, BuyerState>,
    #[account(mut)]
    pub presale_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub buyer_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub buyer: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct PresaleState {
    pub total_tokens_sold: u64,
    pub total_sol_raised: u64,
    pub current_phase: u8,
    pub is_tge_active: bool,
}

#[account]
pub struct BuyerState {
    pub total_allocation: u64,
    pub claimed_amount: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid presale phase.")]
    InvalidPhase,
    #[msg("TGE is not yet active.")]
    TGENotActive,
    #[msg("TGE allocation already claimed.")]
    AlreadyClaimedTGE,
}
