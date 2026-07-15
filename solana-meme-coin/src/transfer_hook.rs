use anchor_lang::prelude::*;
use crate::errors::MemeCoinError;
use anchor_lang::solana_program::sysvar::instructions::{
    load_current_index_checked, load_instruction_at_checked,
};
use anchor_spl::token_2022::spl_token_2022::extension::transfer_hook::TransferHookAccount;
use anchor_spl::token_2022::spl_token_2022::extension::BaseStateWithExtensions;
use spl_tlv_account_resolution::{account::ExtraAccountMeta, state::ExtraAccountMetaList};
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

/// IMPORTANT ARCHITECTURE NOTE (read before touching this file):
///
/// Token-2022's Transfer Hook extension calls this program's `execute`
/// via CPI *during* an in-flight transfer of this same mint. At that
/// point the source token account is flagged mid-transfer, and the
/// SPL Token-2022 program will REJECT any nested transfer of the same
/// mint attempted from inside this hook (reentrancy guard). That means
/// this hook cannot itself CPI the 1% fee to the marketing wallet —
/// there is no way to "siphon" part of the transferred amount from here.
///
/// The enforceable pattern instead is: require a SIBLING instruction,
/// in the same transaction, that pays the fee explicitly. The
/// swap-ui / router is responsible for building that fee-payment
/// instruction alongside the swap when the trade route touches the
/// known DEX pool. This hook's job is only to verify that sibling
/// instruction exists and is correct — and to reject the whole
/// transaction if it's missing, which prevents anyone from stripping
/// the fee instruction out client-side.
///
/// P2P wallet-to-wallet transfers (neither side is the known pool)
/// have no fee-instruction requirement and pass straight through.

pub const TAX_BPS: u64 = 100; // 1%
pub const TAX_BPS_DENOMINATOR: u64 = 10_000;

pub fn initialize_extra_account_meta_list(
    ctx: Context<InitializeExtraAccountMetaList>,
) -> Result<()> {
    // Declares the extra accounts (beyond the standard 4 SPL-Token
    // transfer accounts) that Token-2022 must resolve and pass into
    // `execute` on every transfer of this mint: the dex pool address
    // and marketing wallet, plus the Instructions sysvar itself so
    // we can inspect sibling instructions.
    let account_metas = vec![
        ExtraAccountMeta::new_with_pubkey(&ctx.accounts.dex_pool.key(), false, false)?,
        ExtraAccountMeta::new_with_pubkey(&ctx.accounts.marketing_wallet.key(), false, true)?,
        ExtraAccountMeta::new_with_pubkey(
            &anchor_lang::solana_program::sysvar::instructions::ID,
            false,
            false,
        )?,
    ];

    let account_size = ExtraAccountMetaList::size_of(account_metas.len())? as u64;

    // BUG FIX: this PDA was never actually created before - `payer` was
    // declared in the accounts struct but never used, and `.realloc()`
    // below would fail on a fresh, System-owned, zero-size account since
    // realloc requires the calling program to already own the account.
    // Create it properly here, funded by `payer`, owned by this program.
    let mint_key = ctx.accounts.mint.key();
    let bump = ctx.bumps.extra_account_meta_list;
    let seeds: &[&[u8]] = &[b"extra-account-metas", mint_key.as_ref(), &[bump]];
    let signer_seeds: &[&[&[u8]]] = &[seeds];
    let rent = Rent::get()?;
    let lamports = rent.minimum_balance(account_size as usize);
    anchor_lang::system_program::create_account(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::CreateAccount {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.extra_account_meta_list.to_account_info(),
            },
            signer_seeds,
        ),
        lamports,
        account_size,
        ctx.program_id,
    )?;

    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &account_metas,
    )?;

    Ok(())
}

/// Corrects `dex_pool` after the fact once the real Raydium pool exists -
/// necessary because `initialize_extra_account_meta_list` above must run
/// (and therefore needs SOME dex_pool value) before ANY transfer of this
/// mint can succeed at all, including the presale-phase transfers that
/// happen long before the real pool exists via `seed_liquidity_pool`.
/// Without this instruction, whatever placeholder was used at initial
/// setup would silently remain wrong forever, and the 1% DEX tax - the
/// project's entire revenue model - would never actually activate.
/// Admin-gated: pass the real token vault (the one holding THIS mint,
/// not the pool_state account) from the pool `seed_liquidity_pool` just
/// created.
pub fn update_dex_pool(ctx: Context<UpdateDexPool>) -> Result<()> {
    let account_metas = vec![
        ExtraAccountMeta::new_with_pubkey(&ctx.accounts.dex_pool.key(), false, false)?,
        ExtraAccountMeta::new_with_pubkey(&ctx.accounts.marketing_wallet.key(), false, true)?,
        ExtraAccountMeta::new_with_pubkey(
            &anchor_lang::solana_program::sysvar::instructions::ID,
            false,
            false,
        )?,
    ];
    ExtraAccountMetaList::update::<ExecuteInstruction>(
        &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &account_metas,
    )?;
    Ok(())
}

pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    // Token-2022 sets this flag on both token accounts while a hook-
    // gated transfer is in flight. We don't rely on it for logic here,
    // but asserting it's set is a cheap sanity check that we're really
    // being invoked as a hook and not called directly by a spoofed
    // instruction pretending to be the token program.
    let source_data = ctx.accounts.source.try_borrow_data()?;
    let source_ext = anchor_spl::token_2022::spl_token_2022::extension::StateWithExtensions::<
        anchor_spl::token_2022::spl_token_2022::state::Account,
    >::unpack(&source_data)?;
    let hook_flag = source_ext.get_extension::<TransferHookAccount>()?;
    require!(bool::from(hook_flag.transferring), MemeCoinError::NotInTransfer);
    drop(source_data);

    let is_dex_trade = ctx.accounts.source.key() == ctx.accounts.dex_pool.key()
        || ctx.accounts.destination.key() == ctx.accounts.dex_pool.key();

    if !is_dex_trade {
        msg!("P2P transfer detected — no tax required.");
        return Ok(());
    }

    let required_fee = (amount as u128)
        .checked_mul(TAX_BPS as u128)
        .ok_or(MemeCoinError::MathOverflow)?
        .checked_div(TAX_BPS_DENOMINATOR as u128)
        .ok_or(MemeCoinError::MathOverflow)? as u64;

    // required_fee can legitimately be 0 for dust-sized transfers;
    // in that case there's nothing to enforce.
    if required_fee == 0 {
        return Ok(());
    }

    verify_sibling_fee_instruction(&ctx, required_fee)?;

    msg!(
        "DEX trade verified: {} lamports transferred, {} lamports fee confirmed in sibling instruction.",
        amount,
        required_fee
    );

    Ok(())
}

/// Scans every other instruction in the current transaction (via the
/// Instructions sysvar) looking for an SPL-Token-2022 TransferChecked
/// instruction that pays at least `required_fee` of this same mint
/// into the marketing wallet. Fails the whole transaction if none is
/// found, which is what actually enforces the tax — Token-2022 will
/// unwind the entire transfer if this hook returns an error.
fn verify_sibling_fee_instruction(ctx: &Context<TransferHook>, required_fee: u64) -> Result<()> {
    let ix_sysvar = &ctx.accounts.instructions_sysvar;
    let current_index = load_current_index_checked(ix_sysvar)?;

    let token_2022_program_id = ctx.accounts.mint.owner;
    let marketing_wallet_key = ctx.accounts.marketing_wallet.key();

    let mut i: u16 = 0;
    loop {
        let ix = match load_instruction_at_checked(i as usize, ix_sysvar) {
            Ok(ix) => ix,
            Err(_) => break, // reached end of instruction list
        };

        if i != current_index
            && ix.program_id == *token_2022_program_id
            && !ix.data.is_empty()
        {
            // TransferChecked discriminator = 12 (spl-token-2022 TokenInstruction enum)
            const TRANSFER_CHECKED_DISCRIMINATOR: u8 = 12;
            if ix.data[0] == TRANSFER_CHECKED_DISCRIMINATOR && ix.data.len() >= 9 {
                let ix_amount = u64::from_le_bytes(ix.data[1..9].try_into().unwrap());
                let pays_marketing_wallet = ix
                    .accounts
                    .iter()
                    .any(|a| a.pubkey == marketing_wallet_key);

                if pays_marketing_wallet && ix_amount >= required_fee {
                    return Ok(());
                }
            }
        }

        i += 1;
        if i > 64 {
            break; // sane upper bound on instructions scanned per tx
        }
    }

    Err(MemeCoinError::MissingFeePayment.into())
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut, address = presale_state.admin @ MemeCoinError::Unauthorized)]
    pub payer: Signer<'info>,
    #[account(seeds = [b"presale_state"], bump)]
    pub presale_state: Account<'info, crate::presale::PresaleState>,
    /// CHECK: PDA owned by this program, seeds validated below, created
    /// manually in the handler (see bug-fix note there) - must NOT be an
    /// existing account when this runs, since we create it fresh.
    #[account(
        mut,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: AccountInfo<'info>,
    /// CHECK: the mint this hook is being attached to
    pub mint: AccountInfo<'info>,
    /// CHECK: known AMM pool address (base or quote vault / pool authority)
    /// - almost certainly a PLACEHOLDER at initial setup time, since the
    /// real pool doesn't exist yet at this point in the flow (see
    /// update_dex_pool below, which must be called once it does).
    pub dex_pool: AccountInfo<'info>,
    /// CHECK: destination for collected tax
    pub marketing_wallet: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateDexPool<'info> {
    #[account(address = presale_state.admin @ MemeCoinError::Unauthorized)]
    pub admin: Signer<'info>,
    #[account(seeds = [b"presale_state"], bump)]
    pub presale_state: Account<'info, crate::presale::PresaleState>,
    /// CHECK: PDA owned by this program, seeds validated - must already
    /// exist (created by initialize_extra_account_meta_list earlier).
    #[account(
        mut,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump
    )]
    pub extra_account_meta_list: AccountInfo<'info>,
    /// CHECK: the mint this hook is attached to
    pub mint: AccountInfo<'info>,
    /// CHECK: the REAL pool token vault (holding this mint specifically,
    /// not the pool_state metadata account) - caller's responsibility to
    /// pass the correct address, this instruction only rewrites the
    /// stored value, it doesn't independently verify pool authenticity.
    pub dex_pool: AccountInfo<'info>,
    /// CHECK: must match whatever was set at initial setup - this
    /// instruction doesn't change the marketing wallet, only dex_pool,
    /// but ExtraAccountMetaList::update rewrites the whole list at once
    /// so the correct existing value must be passed again.
    pub marketing_wallet: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct TransferHook<'info> {
    /// CHECK: validated via StateWithExtensions unpack in the handler
    pub source: AccountInfo<'info>,
    /// CHECK: the mint account, `owner` field gives us the token program id
    pub mint: AccountInfo<'info>,
    /// CHECK: destination token account
    #[account(mut)]
    pub destination: AccountInfo<'info>,
    /// CHECK: source token account owner
    pub owner: AccountInfo<'info>,
    /// CHECK: ExtraAccountMetaList PDA, seeds enforced at init time
    pub extra_account_meta_list: AccountInfo<'info>,
    /// CHECK: known DEX pool address, passed via extra account meta list
    pub dex_pool: AccountInfo<'info>,
    /// CHECK: marketing wallet, passed via extra account meta list
    pub marketing_wallet: AccountInfo<'info>,
    /// CHECK: Instructions sysvar, passed via extra account meta list,
    /// used to inspect sibling instructions in the same transaction
    pub instructions_sysvar: AccountInfo<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_percent_fee_math() {
        let amount: u128 = 1_000_000_000; // 1 token @ 9 decimals
        let fee = amount * TAX_BPS as u128 / TAX_BPS_DENOMINATOR as u128;
        assert_eq!(fee, 10_000_000); // 1%
    }

    #[test]
    fn dust_transfer_rounds_fee_to_zero() {
        let amount: u128 = 50; // sub-lamport-equivalent dust
        let fee = amount * TAX_BPS as u128 / TAX_BPS_DENOMINATOR as u128;
        assert_eq!(fee, 0);
    }
}
