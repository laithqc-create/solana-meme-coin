# Project Progress

## MILESTONE: anchor build passes clean AND produces a real artifact, commit 3204ca3

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
   `lib.rs`, both `[programs.*]` entries in `Anchor.toml`).
2. **Devnet deploy**: `anchor deploy --provider.cluster devnet` (needs a
   funded devnet wallet - `solana airdrop` first).
3. **Fill in `swap-ui/.env`** with the real deployed mint address,
   marketing wallet address, decimals, and RPC URL.
4. **Hardcode real pool vault pubkeys** into `RecordPriceSnapshot`'s account
   constraints in `vesting.rs` - currently accepts ANY two token accounts,
   which is fine for now but must be locked down before real funds are at
   risk. Blocked until liquidity is actually deployed to a pool.
5. **End-to-end test on devnet**: initialize_presale -> buy_tokens ->
   activate_tge -> claim_tge -> initialize_vesting (client-side, 90% of
   allocation) -> finalize_investor_vesting -> wait past cliff ->
   release_vesting -> simulate a 30%+ price drop via record_price_snapshot
   + check_and_trigger_volatility_delay -> confirm the 7-day delay applies
   -> confirm the NEXT month's release is still on the original calendar
   grid (not shifted).
6. `OtcPortal.tsx` / spec section 7's actual on-chain OTC contract: not
   started, not scoped yet - will need its own design pass.
7. Pre-mainnet housekeeping (not urgent, but don't forget):
   - `buy_tokens` in `presale.rs` uses plain `+=`/`*` with no overflow
     checks on the SOL/token amounts - `overflow-checks = true` at the
     workspace level should now catch actual overflows at runtime in debug/
     test builds, but worth an explicit `checked_add`/`checked_mul` pass
     for defense in depth.
   - Get a real securities lawyer review (see compliance gate note above).

## Any blockers or decisions pending
- None blocking further progress - ready to move to devnet deploy whenever
  you want to continue.

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
