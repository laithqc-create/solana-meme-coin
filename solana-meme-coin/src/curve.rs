use crate::errors::MemeCoinError;
use anchor_lang::prelude::*;

/// ============================================================
/// LINEAR BONDING CURVE
/// ============================================================
/// Price rises linearly (in lamports per WHOLE token) from PRICE_START
/// at zero tokens sold to PRICE_END at PRESALE_TARGET_TOKENS sold:
///
///   price(s) = PRICE_START + (PRICE_END - PRICE_START) * s / PRESALE_TARGET_TOKENS
///
/// where `s` is cumulative WHOLE tokens sold (curve math is done entirely
/// in whole-token units, NOT base units with decimals - conversion to base
/// units happens once, at the boundary, right before token amounts are
/// used anywhere else in the program). This keeps every intermediate value
/// far smaller and avoids needing fractional lamport-per-base-unit prices.
///
/// PRICE_START/PRICE_END preserve the original spec's presale price bounds
/// ($0.00200 -> $0.00250, expressed here directly as a SOL-denominated
/// lamports-per-token rate since the program has no USD price oracle):
///   500 tokens/SOL at start  == 1e9 lamports / 500 == 2,000,000 lamports/token
///   400 tokens/SOL at end    == 1e9 lamports / 400 == 2,500,000 lamports/token
pub const PRESALE_TARGET_TOKENS: u128 = 300_000_000; // whole tokens, 30% of supply
pub const PRICE_START_LAMPORTS: u128 = 2_000_000;
pub const PRICE_END_LAMPORTS: u128 = 2_500_000;

/// SAFETY CAP: rejects any single purchase requesting more SOL than this in
/// one instruction call. This is NOT a business requirement from the spec -
/// it's a defensive bound so intermediate u128 products in the curve math
/// can never approach overflow regardless of what a malicious or malformed
/// client sends, and so no single transaction can single-handedly buy an
/// outsized fraction of the presale. Well above any plausible legitimate
/// purchase; raise only with real justification, not as a quick unblock.
pub const MAX_SOL_PER_PURCHASE_LAMPORTS: u128 = 10_000 * 1_000_000_000; // 10,000 SOL

/// Result of a curve purchase calculation - both fields matter: the caller
/// must charge exactly `cost_lamports`, not the amount the buyer requested,
/// since a purchase near the presale cap may be partially filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurvePurchase {
    pub tokens_out_whole: u128,
    pub cost_lamports: u128,
}

/// Given `sol_in_lamports` a buyer wants to spend and `cumulative_sold`
/// whole tokens already sold, computes how many whole tokens that buys
/// under the curve, and the exact lamport cost to charge.
///
/// If the uncapped purchase would cross PRESALE_TARGET_TOKENS, the result
/// is capped to exactly the remaining supply, and `cost_lamports` reflects
/// only the true cost of that smaller amount (computed via the forward
/// integral, not the buyer's original `sol_in_lamports`) - the caller must
/// charge only `cost_lamports`, never the full requested amount, in that
/// case. This is what makes "sold out at exactly 300,000,000 tokens" a
/// deterministic, exact trigger rather than an approximation.
pub fn calculate_purchase(
    sol_in_lamports: u64,
    cumulative_sold: u128,
) -> Result<CurvePurchase> {
    require!(sol_in_lamports > 0, MemeCoinError::ZeroAmount);
    require!(
        (sol_in_lamports as u128) <= MAX_SOL_PER_PURCHASE_LAMPORTS,
        MemeCoinError::PurchaseExceedsMaxPerTx
    );
    require!(
        cumulative_sold < PRESALE_TARGET_TOKENS,
        MemeCoinError::PresaleSoldOut
    );

    let remaining = PRESALE_TARGET_TOKENS - cumulative_sold;

    let uncapped_tokens_out =
        tokens_for_sol_floor(sol_in_lamports as u128, cumulative_sold)?;

    if uncapped_tokens_out < remaining {
        // Normal case: buyer's full SOL amount is used, no cap needed.
        Ok(CurvePurchase {
            tokens_out_whole: uncapped_tokens_out,
            cost_lamports: sol_in_lamports as u128,
        })
    } else {
        // Capped case: this purchase would sell out (or overshoot) the
        // presale. Fill exactly the remaining supply and charge only its
        // true cost under the curve, computed via the forward integral.
        let exact_cost = cost_for_tokens_ceil(cumulative_sold, remaining)?;
        require!(
            exact_cost <= sol_in_lamports as u128,
            MemeCoinError::MathOverflow // sanity check: capped cost must never exceed what was offered
        );
        Ok(CurvePurchase {
            tokens_out_whole: remaining,
            cost_lamports: exact_cost,
        })
    }
}

/// Inverse of the curve integral: given lamports to spend and the current
/// cumulative sold, returns whole tokens purchasable, ROUNDED DOWN. Rounding
/// down here means a buyer never receives more tokens than their SOL truly
/// entitles them to under the curve - the protocol is never shortchanged by
/// this rounding direction, only ever the buyer, by less than one token.
///
/// Derivation: cost(s, ds) = P0*ds + (P1-P0)/(2X) * (2*s*ds + ds^2)
/// Solve for ds given cost = sol_in:
///   A*ds^2 + B*ds - Cc = 0
///   A  = (P1 - P0)
///   B  = 2 * (X*P0 + A*s)
///   Cc = 2 * X * sol_in
///   ds = floor[ (-B + sqrt(B^2 + 4*A*Cc)) / (2*A) ]
fn tokens_for_sol_floor(sol_in_lamports: u128, s: u128) -> Result<u128> {
    let x = PRESALE_TARGET_TOKENS;
    let p0 = PRICE_START_LAMPORTS;
    let p1 = PRICE_END_LAMPORTS;
    let a = p1 - p0; // always >= 0 for a non-decreasing curve

    if a == 0 {
        // Degenerate flat-price case (P0 == P1): no quadratic needed.
        return sol_in_lamports
            .checked_div(p0)
            .ok_or(MemeCoinError::MathOverflow.into());
    }

    let b = 2u128
        .checked_mul(
            x.checked_mul(p0)
                .ok_or(MemeCoinError::MathOverflow)?
                .checked_add(a.checked_mul(s).ok_or(MemeCoinError::MathOverflow)?)
                .ok_or(MemeCoinError::MathOverflow)?,
        )
        .ok_or(MemeCoinError::MathOverflow)?;

    let cc = 2u128
        .checked_mul(x)
        .ok_or(MemeCoinError::MathOverflow)?
        .checked_mul(sol_in_lamports)
        .ok_or(MemeCoinError::MathOverflow)?;

    let discriminant = b
        .checked_mul(b)
        .ok_or(MemeCoinError::MathOverflow)?
        .checked_add(
            4u128
                .checked_mul(a)
                .ok_or(MemeCoinError::MathOverflow)?
                .checked_mul(cc)
                .ok_or(MemeCoinError::MathOverflow)?,
        )
        .ok_or(MemeCoinError::MathOverflow)?;

    let sqrt_disc = integer_sqrt(discriminant);

    // sqrt_disc >= b is guaranteed here since discriminant >= b*b (cc, a > 0),
    // so this subtraction cannot underflow.
    let numerator = sqrt_disc.checked_sub(b).ok_or(MemeCoinError::MathOverflow)?;
    let denominator = 2u128.checked_mul(a).ok_or(MemeCoinError::MathOverflow)?;

    numerator
        .checked_div(denominator)
        .ok_or(MemeCoinError::MathOverflow.into())
}

/// Forward integral: exact lamport cost to buy `delta_s` whole tokens
/// starting from cumulative `s`, ROUNDED UP. Only used for the capped
/// (sell-out) case, where delta_s is already known exactly (the remaining
/// supply) and we need its precise cost. Rounding up means the protocol is
/// never shortchanged by a fractional-lamport rounding error - the buyer
/// pays at most one extra lamport versus the true continuous-curve cost.
///
/// cost(s, ds) = P0*ds + (P1-P0)/(2X) * (2*s*ds + ds^2)
fn cost_for_tokens_ceil(s: u128, delta_s: u128) -> Result<u128> {
    let x = PRESALE_TARGET_TOKENS;
    let p0 = PRICE_START_LAMPORTS;
    let p1 = PRICE_END_LAMPORTS;
    let a = p1 - p0;

    let linear_term = p0.checked_mul(delta_s).ok_or(MemeCoinError::MathOverflow)?;

    let two_s_ds = 2u128
        .checked_mul(s)
        .ok_or(MemeCoinError::MathOverflow)?
        .checked_mul(delta_s)
        .ok_or(MemeCoinError::MathOverflow)?;
    let ds_squared = delta_s.checked_mul(delta_s).ok_or(MemeCoinError::MathOverflow)?;
    let bracket = two_s_ds.checked_add(ds_squared).ok_or(MemeCoinError::MathOverflow)?;

    let curve_term_numerator = a.checked_mul(bracket).ok_or(MemeCoinError::MathOverflow)?;
    let two_x = 2u128.checked_mul(x).ok_or(MemeCoinError::MathOverflow)?;

    // Ceiling division: (num + denom - 1) / denom
    let curve_term = curve_term_numerator
        .checked_add(two_x.checked_sub(1).ok_or(MemeCoinError::MathOverflow)?)
        .ok_or(MemeCoinError::MathOverflow)?
        .checked_div(two_x)
        .ok_or(MemeCoinError::MathOverflow)?;

    linear_term
        .checked_add(curve_term)
        .ok_or(MemeCoinError::MathOverflow.into())
}

/// Standard Newton's-method integer square root, floor(sqrt(n)). Used only
/// on values already bounded well within u128 range by MAX_SOL_PER_PURCHASE_LAMPORTS
/// and PRESALE_TARGET_TOKENS - see the module-level overflow analysis in
/// PROGRESS.md / commit message for the specific bound check.
fn integer_sqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_at_zero_matches_start_price() {
        // Buying a tiny amount at s=0 should cost ~PRICE_START per token.
        let purchase = calculate_purchase(PRICE_START_LAMPORTS as u64, 0).unwrap();
        assert_eq!(purchase.tokens_out_whole, 1);
        assert_eq!(purchase.cost_lamports, PRICE_START_LAMPORTS);
    }

    #[test]
    fn full_curve_total_cost_matches_closed_form_integral() {
        // Total cost to buy the ENTIRE presale from s=0 is the definite
        // integral of price(s) from 0 to X, which for a linear curve has
        // the simple closed form: X * (P0 + P1) / 2 (average price * quantity).
        let expected_total = PRESALE_TARGET_TOKENS * (PRICE_START_LAMPORTS + PRICE_END_LAMPORTS) / 2;
        let cost = cost_for_tokens_ceil(0, PRESALE_TARGET_TOKENS).unwrap();
        // Allow at most a few lamports of rounding drift from the ceiling division.
        assert!(cost >= expected_total);
        assert!(cost - expected_total < 10);
    }

    #[test]
    fn price_rises_as_more_tokens_sold() {
        let early = calculate_purchase(1_000_000_000, 0).unwrap(); // 1 SOL at s=0
        let late = calculate_purchase(1_000_000_000, PRESALE_TARGET_TOKENS - 1_000).unwrap(); // 1 SOL near sellout
        assert!(
            late.tokens_out_whole < early.tokens_out_whole,
            "later purchases must receive fewer tokens per SOL (price is higher)"
        );
    }

    #[test]
    fn purchase_near_target_caps_exactly_at_target() {
        let near_target = PRESALE_TARGET_TOKENS - 100;
        // Offer far more SOL than needed to buy the remaining 100 tokens.
        let purchase = calculate_purchase(1_000_000_000_000, near_target).unwrap();
        assert_eq!(purchase.tokens_out_whole, 100);
        assert!(purchase.cost_lamports < 1_000_000_000_000, "must only charge the true cost, not the full offer");
    }

    #[test]
    fn already_sold_out_rejects() {
        let result = calculate_purchase(1_000_000_000, PRESALE_TARGET_TOKENS);
        assert!(result.is_err());
    }

    #[test]
    fn zero_sol_rejects() {
        let result = calculate_purchase(0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn oversized_single_purchase_rejects() {
        let result = calculate_purchase(
            (MAX_SOL_PER_PURCHASE_LAMPORTS + 1) as u64,
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn integer_sqrt_known_values() {
        assert_eq!(integer_sqrt(0), 0);
        assert_eq!(integer_sqrt(1), 1);
        assert_eq!(integer_sqrt(4), 2);
        assert_eq!(integer_sqrt(15), 3); // floor(sqrt(15)) == 3
        assert_eq!(integer_sqrt(16), 4);
        assert_eq!(integer_sqrt(u128::MAX), 18446744073709551615); // floor(sqrt(2^128 - 1))
    }

    #[test]
    fn rounding_never_favors_buyer_over_many_small_purchases() {
        // Simulate many small sequential purchases and confirm the sum of
        // tokens received never exceeds what buying that same total SOL in
        // one shot would yield - i.e. rounding-down-per-purchase can only
        // ever cost the buyer dust, never let them extract extra tokens
        // through repeated small buys.
        let mut cumulative = 0u128;
        let mut total_tokens = 0u128;
        let mut total_spent = 0u128;
        for _ in 0..50 {
            let purchase = calculate_purchase(10_000_000, cumulative).unwrap(); // 0.01 SOL each
            cumulative += purchase.tokens_out_whole;
            total_tokens += purchase.tokens_out_whole;
            total_spent += purchase.cost_lamports;
        }
        let single_shot = calculate_purchase(total_spent as u64, 0).unwrap();
        assert!(
            total_tokens <= single_shot.tokens_out_whole,
            "splitting into many small purchases must never yield MORE tokens than one large purchase for the same total SOL"
        );
    }
}
