# Project Progress

## MILESTONE: Smooth bonding curve built, tested, and CI-verified - commit 40b6ee4

Run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/29127684331
Both jobs green, artifact ~103KB.

**Phase B, first piece done**: `curve.rs` replaces the old 3-tier stepped
presale pricing with a smooth linear bonding curve (user's explicit choice
over keeping the stepped model). Price rises continuously in lamports-per-
token from the original phase-1 start price to phase-3 end price as
cumulative tokens sold goes 0 -> 300,000,000. All math is u128 integer
arithmetic (quadratic formula + integer sqrt for buy-side, forward integral
for the capped/sellout side), fully checked, with 9 unit tests including a
rounding-direction invariant and an exact sellout-boundary test.

Sell-out trigger (user's choice): exactly "all 300M tokens sold" sets
`presale_state.sold_out = true`. A purchase that would overshoot is capped
to exactly the remaining supply and charged only its true cost - this is
what makes sellout deterministic rather than approximate.

TWO REAL BUGS CAUGHT BY CI'S TEST RUN (not just compile - actual test
execution), both now fixed:
1. `integer_sqrt` overflowed at u128::MAX (initial Newton's-method guess
   added 1 to an already-maxed value). Fixed with an overflow-safe guess.
2. A test asserted a wrong mathematical assumption (that spending exactly
   the starting price buys exactly 1 token) - the CODE was actually correct
   here (continuous curves require averaging price across the marginal
   range, so the true first-token cost is fractionally above the nominal
   start price); the TEST was wrong and got corrected to a proper
   round-trip boundary check instead of weakening the code's precision.

ALSO FIXED (found while rewriting buy_tokens, unrelated to the curve
itself): `claimed_amount`/`vesting_funded` were being unconditionally reset
to 0/false on EVERY buy_tokens call, including repeat purchases - this
would have let a buyer claim TGE tokens, buy again, and claim TGE a SECOND
time (claim_tge only guards on claimed_amount == 0). Real double-claim bug,
now fixed: these fields only get their zero value once, at account
creation.

**NOT YET BUILT** (this is the next piece of Phase B, still needed): the
actual instruction that reads `sold_out` and executes the CPI to seed a
real Raydium CPMM or Meteora Dynamic AMM pool with the collected presale
funds + the 30% DEX-liquidity token allocation. Needs an AMM choice from
the user before starting (spec allows either) - not yet asked/decided.

**Audit flag**: this curve module is the single highest-priority piece for
professional security review before mainnet, more so than anything else in
the repo - it's real-money-handling math with rounding-direction and
overflow properties that are easy to get subtly wrong (as the two bugs
above demonstrate, even with careful intent).

---



Both CI jobs green, with a real downloadable artifact this time:
https://github.com/laithqc-create/solana-meme-coin/actions/runs/28974879136
(anchor-build-output, ~100KB - the compiled .so + IDL)

Note: the PREVIOUS "green" run (90d5db5) actually had a broken artifact
path (pointed at solana-meme-coin/target/deploy/ when the real output was
at repo-root target/deploy/, since the workspace Cargo.toml moved target/
to the root) - it was silently swallowed by if-no-files-found: ignore.
Fixed and changed to `warn` so this won't go silent again.

This is the first time the Rust program has compiled at all this session -
started from zero (repo had no Anchor.toml, no root Cargo.toml, wrong
anchor-spl version, wrong feature flags, missing trait imports, and three
conflicting error enums). All fixed and verified on GitHub's real runners.

## What's been completed

### Compliance gate (resolved, earlier session)
- Automated volume-generation / wash-trading bot: REJECTED, dropped from
  scope permanently.
- Jurisdiction: user confirmed global public presale, no KYC/geofencing,
  anonymous team, mint+admin authority destroyed. User explicitly
  acknowledged the securities-law risk this creates, in writing. Not
  blocked on my end - user's legal risk to own - but stays visible here.
  Recommend a real securities lawyer review before mainnet / real funds.

### Rust program - CODE COMPLETE AND CI-VERIFIED
- `vesting.rs`, `transfer_hook.rs`, `presale.rs` all rewritten this session
  (see git log on branch `claude-session-fixes` for full detail on each).
- `errors.rs` (NEW) - single shared `MemeCoinError` enum. Anchor only allows
  ONE #[error_code] enum per program; the three files each had their own,
  which passed `cargo check` fine but failed `anchor build` with "Multiple
  error definitions are not allowed." This was the final blocker.
- `Anchor.toml` - created (repo had none). Points `[workspace] members` at
  the existing `solana-meme-coin/` folder instead of moving into the
  conventional `programs/` layout.
- Root `Cargo.toml` - created (repo had none anywhere). Required because
  `anchor build` runs `cargo metadata` from the repo root and needs an
  actual Cargo workspace manifest there - Anchor.toml's `[workspace]
  members` list alone wasn't sufficient. Contains `[profile.release]
  overflow-checks = true`, required by Anchor 0.30+ and must live at the
  workspace root specifically (Cargo ignores `[profile.*]` in non-root
  members).
- `solana-meme-coin/Cargo.toml` - anchor-lang/anchor-spl bumped 0.29.0 ->
  0.30.1 (token_interface module didn't exist before 0.30), added
  `init-if-needed` feature (required by presale.rs's `BuyTokens`, was
  silently breaking that whole Accounts derive), added `idl-build` feature,
  aligned solana-program/spl-token-2022 versions.
- Program ID is still the PLACEHOLDER `TokenVesting1111111111111111111111111111111`
  in both `declare_id!` (lib.rs) and `Anchor.toml`. NOT a real generated
  keypair yet.

### CI (GitHub Actions) - fully working
- `.github/workflows/rust-check.yml`: `cargo-check` (fast) + `anchor-build`
  (slow, full toolchain install) both passing on every push to
  `claude-session-fixes`.

### swap-ui - DONE, fully verified
- Real Phantom/Solflare wallet connection, real swap execution via
  Jupiter's `/v6/swap-instructions`, Token-2022 fee-instruction injection
  matching the transfer hook's sibling-instruction requirement.
- VERIFIED: `npm install`, `tsc --noEmit`, `vitest run` (2/2 pass), `vite
  build` all clean.
- NOT wired: `OtcPortal.tsx` still fully mocked, spec section 7's OTC
  contract doesn't exist in Rust or the UI yet.

## Current file being worked on
- Nothing in progress - just landed a clean, fully-green CI run.

## Exact next steps (in order)
1. **Generate a real program keypair** (`solana-keygen new -o
   target/deploy/solana_meme_coin-keypair.json` after a local `anchor
   build`, or let a fresh `anchor build` generate one), then `anchor keys
   sync` to replace the placeholder ID everywhere (`declare_id!` in
   `lib.rs`, both `[programs.*]` entries in `Anchor.toml`). NOTE: Claude
   should NOT generate this keypair inside a chat sandbox - key custody
   risk. Either the user generates it locally, or via a manually-triggered
   GitHub Actions workflow that uploads it as an authenticated-download
   artifact (never printed in chat/logs beyond the public pubkey).
2. **Devnet deploy**: `anchor deploy --provider.cluster devnet` (needs a
   funded devnet wallet - `solana airdrop` first).
3. **AMM choice needed (Raydium CPMM vs Meteora Dynamic AMM)** - blocks
   building the actual pool-seeding instruction that reads
   `presale_state.sold_out` and CPIs into the chosen AMM to create/fund the
   real liquidity pool with the collected SOL + 30% DEX-liquidity token
   allocation. This is the remaining piece of Phase B (the curve itself is
   done - see milestone above). Ask the user which AMM before starting.
4. **Fill in `swap-ui/.env`** with the real deployed mint address,
   marketing wallet address, decimals, and RPC URL.
5. **Hardcode real pool vault pubkeys** into `RecordPriceSnapshot`'s account
   constraints in `vesting.rs` - currently accepts ANY two token accounts,
   which is fine for now but must be locked down before real funds are at
   risk. Blocked until liquidity is actually deployed to a pool (item 3).
6. **End-to-end test on devnet**: initialize_presale -> buy_tokens
   (repeatedly, exercising the curve across its full range, including the
   exact-sellout capping case) -> activate_tge -> claim_tge ->
   initialize_vesting (client-side, 90% of allocation) ->
   finalize_investor_vesting -> wait past cliff -> release_vesting ->
   simulate a 30%+ price drop via record_price_snapshot +
   check_and_trigger_volatility_delay -> confirm the 7-day delay applies ->
   confirm the NEXT month's release is still on the original calendar grid
   (not shifted).
7. `OtcPortal.tsx` / spec section 7's actual on-chain OTC contract: not
   started, not scoped yet - will need its own design pass.
8. Pre-mainnet housekeeping (not urgent, but don't forget):
   - Get a professional security audit of `curve.rs` specifically before
     mainnet - highest-priority module for review, real-money-handling
     integer math (two real bugs already caught by unit tests this
     session - a good sign the tests work, not a reason to skip audit).
   - Get a real securities lawyer review (see compliance gate note above).

## Any blockers or decisions pending
- **AMM choice (Raydium CPMM vs Meteora)** needed before Phase B can be
  finished - see item 3 above.
- Program keypair not yet generated (item 1) - blocks devnet deploy.

## GitHub / CI state
- Branch: `claude-session-fixes` (NOT merged to main - still needs human
  review of the whole diff before that decision, even though CI is green).
- Latest commit: `90d5db5`.
- Latest CI run (both jobs green):
  https://github.com/laithqc-create/solana-meme-coin/actions/runs/28973415475
- PAT token used this session should be revoked once you're done with this
  round of work.

---
### RESUME FROM HERE
1. Read this file (done, if you're reading it).
2. Decide: merge `claude-session-fixes` to `main` now (CI is green, but a
   human hasn't read the full diff yet), or keep iterating on the branch
   first via devnet deploy (item 2 in "Exact next steps").
3. Either way, next concrete action is generating a real program keypair
   and running `anchor keys sync` (item 1 above) - everything after that
   depends on having a real program ID.
