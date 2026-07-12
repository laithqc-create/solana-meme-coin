# Project Progress

## MILESTONE: AMM decision made (Raydium CPMM) + keypair-generation workflow added

**AMM choice resolved: Raydium CPMM** (over Meteora, superseding the
"Meteora chosen" note in the milestone below - user confirmed this pivot).

Reasoning, checked against current docs rather than memory:
- Raydium publishes an official Anchor Rust crate with a `cpi` feature
  flag - `raydium-cpmm-cpi` (git: `raydium-io/raydium-cpi`) - that covers
  pool CREATION (`InitializeCpmm`), not just swaps. This is a real,
  maintained, program-callable CPI surface, unlike Meteora's classic
  Dynamic AMM v1 (still TS-SDK-only for pool creation - confirmed again,
  no change from the finding below).
- Meteora Dynamic Bonding Curve (DBC) does have an official CPI crate too,
  but it's the wrong shape: DBC is Meteora's OWN on-chain bonding-curve
  program that graduates to a Meteora AMM automatically. Adopting it would
  mean replacing this project's own `curve.rs` + presale/vesting logic
  with Meteora's mechanism, not just plugging in a pool-seeding CPI.
  Rejected for that reason.
- Meteora DAMM v2 is a legitimate second choice (also has an official CPI
  crate) if there's ever a specific reason to prefer Meteora's fee/LP-
  locking model - noted here in case AMM choice needs revisiting later.

**Real dependency cost flagged, not yet paid**: both the Raydium CPMM crate
and Meteora DAMM v2 crate pin to `anchor-lang`/`anchor-spl = "0.32.1"`.
This repo is currently on `0.30.1`. Bumping to 0.32.1 is a real version
jump to test (potential breaking API changes) against the existing
`vesting.rs` / `transfer_hook.rs` / `presale.rs`, not just a Cargo.toml
edit - do this as its own verified step before/alongside writing the
pool-seeding CPI, don't assume it's a no-op.

**Keypair-generation workflow added** (not yet pushed - see below):
`.github/workflows/generate-program-keypair.yml`. Manually-triggered
(`workflow_dispatch`, requires typing "generate" as confirmation) so it
never fires accidentally. Runs on a GitHub-hosted runner: generates the
program keypair, runs `anchor keys sync` to replace the placeholder ID in
`lib.rs`/`Anchor.toml`, commits ONLY that public-ID diff back to the
branch, uploads the private keypair as a 1-day-retention artifact (never
printed to logs beyond the public key). This satisfies the "don't generate
keys inside a chat sandbox" constraint from the previous milestone.

**NOT YET PUSHED**: this session's sandbox has no repo push credentials
(no PAT set up this round). The workflow file is staged locally only.
Either provide a PAT (revoke after use, per standing rule) or copy the
diff in manually via GitHub's web UI before it can actually be triggered.

---



## MILESTONE: Treasury fund-custody gap fixed + Meteora integration scoped - commit b856f78

Run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/29164923735
Both jobs green.

**Real bug fixed**: `treasury` in `buy_tokens` was an unconstrained
`AccountInfo` with zero address/seeds validation - anyone constructing the
transaction could redirect investor SOL to any account. This directly
contradicted the "no human can ever withdraw investor funds" design goal.
Now constrained to a program PDA (`seeds = [b"presale_treasury"]`) -
Anchor's own account validation makes this the only address that will ever
pass, full stop. No behavior change for legitimate buyers.

**Meteora Dynamic AMM integration - researched, NOT yet built, and here's
exactly why:**

User chose Meteora Dynamic AMM (over Raydium CPMM) for the sellout
auto-route. Before writing any CPI code, checked their actual docs/package
registry rather than building from memory - important finding:
- Meteora's NEWER products (DAMM v2, Dynamic Bonding Curve) have official
  Rust CPI crates with a `cpi` feature flag - safe to depend on.
- The CLASSIC Dynamic AMM (v1) - the one actually chosen here - is
  officially supported only via their TypeScript SDK
  (`@meteora-ag/dynamic-amm-sdk`) and off-chain setup scripts
  (`meteora-pool-setup`, run via Bun). Pool CREATION specifically is
  documented as a client-side/off-chain action, not a CPI-from-your-own-
  program operation.

Given: (a) no official Rust CPI crate for pool creation on this specific
product, (b) no live Solana RPC access in the sandbox this was researched
in to fetch/verify their program's real IDL or account layout, and (c) this
is real-money-moving code where a wrong account in the CPI list is a
silent fund-loss bug, not a compile error - decided NOT to hand-reconstruct
the raw CPI from guesswork. User explicitly signed off on this being the
right call rather than pushing for a risky guess.

**Agreed architecture**: on-chain program custodies and correctly tracks
funds (verifiable, testable); the actual Meteora pool creation is driven by
their own maintained SDK/tooling, which is the same integration path every
legitimate Meteora launch uses.

**What's still needed to finish this** (next session or once real Meteora
tooling/RPC access is available):
1. A `seed_liquidity_pool`-type instruction that, gated on
   `presale_state.sold_out == true`, CPIs the collected SOL out of the
   `presale_treasury` PDA (as signer, via `invoke_signed`) into whatever
   Meteora requires for the deposit step. This is the piece that genuinely
   needs their real IDL/CPI account layout in hand - do NOT guess this.
   Possible paths to get there safely:
   a. Find and inspect Meteora's "CPI example for meteora programs" Rust
      repo on GitHub (mentioned in their org listing, ~13 stars) directly,
      rather than reconstructing from doc snippets.
   b. Get RPC/devnet access to fetch their program's actual on-chain IDL
      and verify account ordering against a real transaction.
   c. Have the admin pre-create the pool shell via their official
      TypeScript SDK/script (this part IS well-documented and safe to
      build), and have the on-chain instruction only need to handle a
      simpler, better-documented "deposit into existing pool" CPI rather
      than full pool creation - worth checking if that's a smaller, safer
      surface than full creation.
2. **The 30% DEX-liquidity token allocation isn't custodied anywhere yet
   either** - nothing in the current code mints, reserves, or tracks that
   30% at all. This needs its own instruction (likely at `initialize_presale`
   time, transferring 30% of total supply into a PDA-owned vault
   analogous to `presale_treasury`) before the pool-seeding step has
   tokens to deposit alongside the SOL.

---



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
1. **Push `.github/workflows/generate-program-keypair.yml`** to
   `claude-session-fixes` - currently staged locally only, no repo push
   credentials in the sandbox this session. Provide a PAT (revoke after
   use, per standing rule) or copy the diff in manually via GitHub's web
   UI.
2. **Run the keypair-generation workflow** (manual `workflow_dispatch`,
   type "generate" to confirm). It will: generate the keypair on the
   runner, run `anchor keys sync` to replace the placeholder ID in
   `lib.rs`/`Anchor.toml`, commit that public-ID diff back to the branch,
   and upload the private keypair as a 1-day-retention artifact. Download
   and relocate it to real secret storage immediately, then delete the
   GitHub artifact.
3. **Bump `anchor-lang`/`anchor-spl` 0.30.1 -> 0.32.1** (required by the
   Raydium CPMM CPI crate - see milestone above). Do this as its own
   verified step (`cargo check` + `anchor build` + existing test suite)
   BEFORE writing the pool-seeding CPI, since it's a real version jump
   that could break existing account/trait usage in `vesting.rs`,
   `transfer_hook.rs`, or `presale.rs`.
4. **Devnet deploy**: `anchor deploy --provider.cluster devnet` (needs a
   funded devnet wallet - `solana airdrop` first). Depends on item 2.
5. **Build the `seed_liquidity_pool`-type instruction** using
   `raydium-cpmm-cpi` (git: `raydium-io/raydium-cpi`, `cpi` feature) -
   gated on `presale_state.sold_out == true`, CPIs the collected SOL out of
   the `presale_treasury` PDA (as signer, via `invoke_signed`) plus the 30%
   DEX-liquidity token allocation into a new Raydium CPMM pool
   (`InitializeCpmm`). This is the remaining piece of Phase B (the curve
   itself is done - see earlier milestone).
6. **The 30% DEX-liquidity token allocation still isn't custodied
   anywhere** - needs its own instruction (likely at `initialize_presale`
   time) transferring 30% of total supply into a PDA-owned vault analogous
   to `presale_treasury`, before item 5 has tokens to deposit alongside the
   SOL.
7. **Fill in `swap-ui/.env`** with the real deployed mint address,
   marketing wallet address, decimals, and RPC URL.
8. **Hardcode real pool vault pubkeys** into `RecordPriceSnapshot`'s account
   constraints in `vesting.rs` - currently accepts ANY two token accounts,
   which is fine for now but must be locked down before real funds are at
   risk. Blocked until liquidity is actually deployed to a pool (item 5).
9. **End-to-end test on devnet**: initialize_presale -> buy_tokens
   (repeatedly, exercising the curve across its full range, including the
   exact-sellout capping case) -> activate_tge -> claim_tge ->
   initialize_vesting (client-side, 90% of allocation) ->
   finalize_investor_vesting -> wait past cliff -> release_vesting ->
   simulate a 30%+ price drop via record_price_snapshot +
   check_and_trigger_volatility_delay -> confirm the 7-day delay applies ->
   confirm the NEXT month's release is still on the original calendar grid
   (not shifted).
10. `OtcPortal.tsx` / spec section 7's actual on-chain OTC contract: not
    started, not scoped yet - will need its own design pass.
11. Pre-mainnet housekeeping (not urgent, but don't forget):
    - Get a professional security audit of `curve.rs` specifically before
      mainnet - highest-priority module for review, real-money-handling
      integer math (two real bugs already caught by unit tests this
      session - a good sign the tests work, not a reason to skip audit).
    - Get a real securities lawyer review (see compliance gate note above).

## Any blockers or decisions pending
- **Workflow file not pushed** - no PAT in this session's sandbox (item 1
  above). Needs a PAT or manual copy-in before the keypair can be
  generated.
- Program keypair not yet generated (item 2) - blocks devnet deploy and
  everything downstream of it.
- ~~AMM choice~~ RESOLVED this session: Raydium CPMM (see milestone above).

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
2. AMM choice is RESOLVED: Raydium CPMM. Do not re-litigate this unless
   something material changes (e.g. Raydium deprecates the CPI crate) -
   see top milestone for full reasoning.
3. Immediate blocker: `.github/workflows/generate-program-keypair.yml` is
   drafted but NOT pushed (no PAT this session). Get it onto
   `claude-session-fixes` first (PAT or manual copy-in).
4. Then run that workflow (`workflow_dispatch`, type "generate") to get a
   real program ID - everything else (devnet deploy, the anchor-lang 0.32.1
   bump, the Raydium CPMM pool-seeding instruction) depends on that.
5. Separate open decision, still pending: merge `claude-session-fixes` to
   `main` now (CI green, but no human full-diff review yet) vs. keep
   iterating on the branch through devnet deploy first. Not blocking -
   revisit whenever convenient.
