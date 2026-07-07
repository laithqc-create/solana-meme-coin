# Project Progress

## What's been completed

### Compliance gate (resolved, earlier session)
- Automated volume-generation / wash-trading bot from the original spec doc:
  REJECTED as ToS/market-manipulation violation, permanently dropped.
- Jurisdiction: user confirmed global public presale, no KYC/geofencing,
  anonymous team, mint+admin authority destroyed. User explicitly
  acknowledged in writing the securities-law risk this creates. Not blocked
  on my end — user's legal risk to own — but must stay visible here so it's
  never silently forgotten. Recommend a real securities lawyer review before
  mainnet / real funds move.

### Rust program — CODE COMPLETE, PARTIALLY CI-VERIFIED
- `vesting.rs` — full rewrite: real cliff(30d)+8%/month release math,
  pool-vault-based 48h volatility check, 7-day delay on 30%+ drop that only
  affects the currently-maturing tranche (doesn't shift future grid). Unit
  tests included.
- `transfer_hook.rs` — full rewrite with an architecture correction: the
  original stub assumed the hook could CPI the 1% tax directly, which
  Token-2022 blocks (reentrancy guard on same-mint nested transfers). Real
  implementation uses sibling-instruction verification via the Instructions
  sysvar — hook rejects the whole tx if a DEX-side transfer doesn't have a
  matching fee-payment instruction to the marketing wallet in the same tx.
- `presale.rs` — migrated from legacy `anchor_spl::token` to
  `token_interface` (Token-2022 compatible). Added `finalize_investor_vesting`
  wiring buyer allocations to `vesting.rs` (two-step: client calls
  `vesting::initialize_vesting` then `presale::finalize_investor_vesting`,
  which cross-validates beneficiary/mint/amount before funding it). Added
  `activate_tge` admin instruction + `admin: Pubkey` on `PresaleState` (was
  missing entirely — `claim_tge` could never have succeeded before this).
- `lib.rs` — instruction list updated to match all of the above.
- `Anchor.toml` — created (repo had NONE at all). Points `[workspace]
  members` at the existing `solana-meme-coin/` folder rather than moving
  files into the conventional `programs/` layout — deliberate choice to
  avoid a large risky move; can revisit later.
- `Cargo.toml` — went through several real, CI-verified fixes this session:
  1. `token_2022_extensions` isn't a real feature on anchor-spl 0.29.0 (was
     a bad guess) → removed.
  2. `anchor_spl::token_interface` (used throughout all three rewritten
     files) doesn't exist before Anchor 0.30.0 at all → bumped
     anchor-lang/anchor-spl 0.29.0 → 0.30.1, aligned solana-program (1.17.0
     → 1.18.17) and spl-token-2022 (2.0.0 → 3.0.0) to compatible versions,
     added required `idl-build` feature and `[profile.release]
     overflow-checks = true` (both newly required by Anchor 0.30+).
  3. `init_if_needed` constraint in `presale.rs`'s `BuyTokens` needs an
     explicit anchor-lang feature flag → added `features = ["init-if-needed"]`.
     (This one bug was cascading into ~8 unrelated-looking `Bumps`/`Accounts`
     trait errors — all fixed by this single feature flag.)
  4. `transfer_hook.rs` called `.get_extension()` without importing
     `BaseStateWithExtensions`, the trait it's defined on → added the import,
     also dropped an unused `Seed` import flagged as a warning.

### CI (GitHub Actions) — set up this session, actively in use
- `.github/workflows/rust-check.yml`: two jobs.
  - `cargo-check`: fast, `cargo check` + `cargo test --lib` on every push.
    **CURRENTLY PASSING** (run 28892712897, commit 6074519) — this is real
    verification that the Rust code is at least type-correct and the unit
    tests pass.
  - `anchor-build`: slower, installs Solana CLI + Anchor via avm, then runs
    `anchor build` (the real BPF-target build). **CURRENTLY FAILING**
    (same run) — but importantly, ALL the setup steps succeeded (Solana CLI
    install, Anchor CLI install, throwaway wallet generation) — the failure
    is in the `anchor build` step itself, meaning it's likely a real
    BPF-target-specific code/config issue, not flaky environment setup.
  - **COULD NOT READ THE EXACT ERROR** — GitHub serves job logs from Azure
    Blob Storage under a signed URL, and that host isn't on this sandbox's
    allowed network egress list (only github.com/api.github.com are). Every
    attempt (curl direct, curl -L, web_fetch) got blocked with
    `x-deny-reason: host_not_allowed`. This is a sandbox limitation, not
    something fixable by retrying.

### swap-ui — DONE, fully verified (Node toolchain available, unlike Rust)
- Was previously quote-only with zero wallet adapters and a non-functional
  swap button. Now: real Phantom/Solflare wallet connection, real swap
  execution via Jupiter's `/v6/swap-instructions` (not the pre-built-tx
  endpoint, specifically so the fee instruction could be spliced in before
  compiling), Token-2022 `transferChecked` fee instruction sized correctly
  per direction (exact for sells, quote+0.1% buffer for buys — see comment
  in `src/lib/swapTransaction.ts` for why).
- VERIFIED: `npm install`, `tsc --noEmit` clean, `vitest run` 2/2 pass,
  `vite build` clean production build.
- NOT wired: `OtcPortal.tsx` still fully mocked, spec section 7's real OTC
  contract doesn't exist in Rust or the UI. Not touched.
- Needs a `.env` with real mint/marketing-wallet addresses once the program
  is actually deployed (doesn't exist yet — placeholder program ID only).

## Current file being worked on
- Just pushed commit `6074519` on branch `claude-session-fixes`. Waiting on
  the `anchor build` job's actual error text.

## Exact next steps (in order)
1. **Get the `anchor build` error text.** Either:
   - Paste it from https://github.com/laithqc-create/solana-meme-coin/actions/runs/28892712897/job/85709681565
     (expand the red "anchor build" step), or
   - A future Claude session with different network egress rules might be
     able to fetch the blob storage log directly — worth trying
     `web_fetch`/`curl` on the signed URL again in case sandbox config
     differs next time, before assuming it's still blocked.
2. Fix whatever that error says. Given cargo-check passes, this is very
   likely one of: BPF stack-size limits (Solana programs have an 8KB stack
   limit that plain `cargo check` doesn't enforce, but `build-sbf` does —
   the price_history ring buffer in `vesting.rs` or the multi-account
   structs could plausibly be stack-heavy — check for `Box<>`-wrapping
   needs), OR a cfg/target mismatch specific to `build-sbf`'s toolchain
   pinning vs the `stable` Rust toolchain the cargo-check job uses (worth
   trying `solana-cli`'s pinned Rust version instead of `dtolnay/rust-toolchain@stable`
   for the anchor-build job specifically).
3. Once `anchor build` passes: devnet deploy (`anchor deploy` or manual
   `solana program deploy`), get a REAL program ID, run `anchor keys sync`
   (or manually update `declare_id!` in `lib.rs` + `Anchor.toml`'s
   `[programs.*]` — currently both use the placeholder
   `TokenVesting1111111111111111111111111111111`).
4. Fill in `swap-ui/.env` with real mint/marketing-wallet addresses.
5. Hardcode real pool vault pubkeys into `RecordPriceSnapshot`'s account
   constraints in `vesting.rs` (currently accepts ANY two token accounts —
   needs the real pool address once liquidity is actually deployed).
6. End-to-end test: presale → TGE claim → cliff → first monthly release →
   simulated 30% price drop → confirm 7-day delay → confirm next month's
   grid position unaffected.
7. `OtcPortal.tsx` / spec section 7's actual OTC contract: not started,
   not scoped yet.

## Any blockers or decisions pending
- Waiting on the `anchor build` error text (item 1 above) — cannot proceed
  meaningfully on the Rust side without it.
- Real pool vault addresses not yet known (item 5) — blocked on deployment.
- `buy_tokens` in `presale.rs` uses plain `+=`/`*` with no overflow checks
  — pre-existing, not touched this session, worth a pass before mainnet.

## GitHub / CI state
- Branch: `claude-session-fixes` (NOT merged to main — intentional, needs
  human review + a passing anchor-build before that's even worth
  considering).
- Latest pushed commit: `6074519`.
- Latest CI run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/28892712897
  — cargo-check: SUCCESS. anchor-build: FAILURE (error text unknown).
- PAT tokens used this session are session-scoped and should be revoked
  after use, same as prior sessions — check whether the current one is
  still needed before closing this out.

---
### RESUME FROM HERE
1. Read this file top to bottom first (you're doing that now).
2. Get the anchor-build error from the run link above — try fetching it
   directly first in case sandbox network rules differ; fall back to
   asking the user to paste it if blocked again.
3. Fix, commit to `claude-session-fixes`, push, watch CI, repeat until
   anchor-build passes.
4. Then move to devnet deploy (step 3 in "Exact next steps" above).
