# Project Progress

## MILESTONE: seed_liquidity_pool CI-CONFIRMED - both cargo check AND anchor build passed, first attempt

Run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/29367477487
(overall conclusion: `success`, confirmed via Actions API directly)

The CPI account struct wiring, PDA seed-formula verification logic, and
token_0/token_1 runtime-ordering branch all compiled correctly against the
real Solana BPF toolchain on the first real attempt - the upfront research
(cloning raydium-io/raydium-cpi and reading actual source rather than
guessing) paid off here compared to the anchor-lang 0.32.1 bump earlier,
which took 4 real CI round-trips.

**Still NOT done - this is compile-verified only, not behavior-verified**:
no test has actually invoked this instruction against a running validator
or devnet cluster yet. Real remaining risks that only a live invocation
would catch: whether the actual on-chain Raydium program's own internal
validation accepts what we're sending (our manual PDA checks replicate
the formula but don't guarantee we've got init_amount_0/init_amount_1 or
open_time semantics exactly right), and whether the WSOL-wrapping +
sync_native sequence produces the exact balance Raydium's inner
instruction expects to pull. This needs a real devnet invocation (or at
minimum a local-validator anchor test with cloned Raydium accounts, which
Raydium's own docs describe doing for exactly this kind of integration
testing) before treating it as functionally correct, not just compiling.

## MILESTONE: seed_liquidity_pool instruction written - real Raydium interface verified from source, NOT CI-tested yet

**Verification method** (per defi-blueprint skill - never invent library
API details from memory): cloned `raydium-io/raydium-cpi` directly into
the sandbox (`git clone`, not a blog/memory) and read the actual
`programs/cpmm-cpi/src/{lib.rs,context.rs,states.rs}` source. Confirmed:
- Real package name `raydium-cpmm-cpi` has `[lib] name = "raydium_cpmm_cpi"`
  - NOT `raydium_cp_swap`, which several older/community examples for
  earlier Anchor versions still use for what turns out to be a different,
  superseded crate.
- Real instruction name is `initialize` (`Context<Initialize>`), not
  `InitializeCpmm` - that name only appears in an unofficial third-party
  "pinocchio" helper crate, not Raydium's own.
- Got the exact real `Initialize` accounts struct field-by-field
  (`creator, amm_config, authority, pool_state, token_0_mint, token_1_mint,
  lp_mint, creator_token_0, creator_token_1, creator_lp_token,
  token_0_vault, token_1_vault, create_pool_fee, observation_state,
  token_program, token_0_program, token_1_program,
  associated_token_program, system_program, rent`) and the real PDA seed
  formulas for every Raydium-owned account (`POOL_SEED`,
  `POOL_LP_MINT_SEED`, `POOL_VAULT_SEED`, `OBSERVATION_SEED`, `AUTH_SEED`).

**Design decisions made, with reasoning**:
- Raydium enforces `token_0_mint.key() < token_1_mint.key()` - not knowable
  at compile time (depends on our specific deployed mint's pubkey vs
  WSOL's fixed pubkey), so ALL Raydium-owned PDA accounts are
  `UncheckedAccount` in our struct (matching Raydium's own official CPI
  example's convention) and verified imperatively in the instruction body
  via `Pubkey::find_program_address`, branching on the actual runtime
  comparison - not via Anchor's declarative `seeds = [...]`, which can't
  easily express conditional ordering.
- New `dex_liquidity_vault` (30% DEX allocation) added to
  `InitializePresale`, funded off-chain by the admin exactly like
  `presale_vault` already is (no on-chain minting exists anywhere in this
  program - confirmed via grep before assuming otherwise).
- New shared `pool_creator_authority` PDA (empty, seeds only) added
  because Raydium's `creator` field must be both a Signer (via
  `invoke_signed`) AND the token::authority of BOTH funding vaults
  simultaneously - a self-authorizing vault (like presale_vault's own
  pattern) can't satisfy that for two DIFFERENT vaults at once.
- Presale's collected SOL (in `presale_treasury`, a plain system-owned
  PDA) is wrapped into WSOL via a direct lamport transfer +
  `sync_native` - the whole treasury balance is drained, since nothing
  needs that PDA to survive afterward.
- `amm_config` and `create_pool_fee` are passed in as caller-supplied
  accounts, NOT hardcoded - two different sources gave inconsistent
  devnet program IDs for Raydium CPMM (raydium-cpi-example's README says
  `CPMDWBwJDtYax9qW7AyRuVC19Cc4L4Vcy4n2BHAbHkCW`, while raydium-cpmm-cpi's
  own `declare_id!` under its `devnet` feature says
  `DRaycpLY18LhpbydsBWbVJtxpNv9oXPgjRSfpF2bWpYb`) - rather than guess which
  is current, the caller supplies the correct addresses for whatever
  cluster/deployment they're actually targeting.
- Added `PresaleState.liquidity_seeded: bool` to prevent double-calling
  (updated space calc, `initialize_presale` sets it false).
- New error variants in `errors.rs`: `PresaleNotSoldOut`,
  `LiquidityAlreadySeeded`, and one mismatch variant per manually-verified
  Raydium PDA (`PoolStateMismatch`, `LpMintMismatch`, `TokenVaultMismatch`,
  `ObservationStateMismatch`, `RaydiumAuthorityMismatch`).

**Known open assumption, flagged for review, not yet verified**:
`open_time` is passed as a caller-supplied parameter rather than hardcoded
- common convention elsewhere is `0` for "tradeable immediately," but this
  wasn't independently confirmed against Raydium's actual runtime
  behavior for that value. Low risk (a timestamp gate, not a fund-custody
  parameter), but worth double-checking before relying on it.

**NOT yet verified**: local `cargo check` only confirmed the dependency
graph resolves (including the new `raydium-cpmm-cpi` git dependency) -
the sandbox's `rustc 1.75` hits the same known MSRV ceiling as before
(`borsh` needing `1.77`, which real CI's `1.84.1` already satisfies)
before ever reaching our own code. **This instruction's actual
compilation has not been tested yet** - next action is pushing to CI and
treating the first result as a real, likely-imperfect first attempt, not
a finished implementation.

Deployed via `.github/workflows/deploy-devnet.yml`, run against commit
`5409e05` on `claude-session-fixes`. Verified via a live query against
devnet's own RPC (`solana program show`, not just trusting the deploy
command's exit code):

```
Program Id: EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N
Owner: BPFLoaderUpgradeab1e11111111111111111111111
ProgramData Address: 22v5EueXp1b2Xp9DAYoUiQWB2X9RdnAfq6dVsN2wAQdn
Authority: B4oEVq6jSzqjJz9wXjDPSgBToK9BTw8LTjfQYrXd4Egw
Last Deployed In Slot: 476186173
Data Length: 426640 (0x68290) bytes
Balance: 2.97061848 SOL
```

`Owner: BPFLoaderUpgradeab1e11111111111111111111111` confirms this is a
real upgradeable program (not a placeholder/empty account).
`Authority: B4oEVq6jSzqjJz9wXjDPSgBToK9BTw8LTjfQYrXd4Egw` correctly
matches the persistent devnet payer wallet. `Data Length: 426640 bytes`
confirms actual compiled bytecode landed on-chain.

**Real path this took** (worth remembering for the mainnet version of this
same workflow later):
1. First devnet-deploy workflow attempt used ephemeral generate+airdrop
   payer - failed 10/10 airdrop attempts, GitHub-hosted runners' shared
   IPs are rate-limited by devnet's CLI airdrop endpoint.
2. Switched to a PERSISTENT payer wallet (keypair generated locally,
   funded once via the web faucet at faucet.solana.com - captcha-based,
   not IP-limited - then stored as a `DEVNET_PAYER_KEYPAIR` repo secret).
   This is now reusable for every future devnet deploy, no more
   rate-limiting fights.
3. First run with the persistent payer still failed - workflow jumped
   straight to `anchor deploy` without ever running `anchor build` first
   (fresh runner checkout each time, no cached `.so`). Added the missing
   build step.
4. Second run with the build step: SUCCESS, fully verified above.

This completes item 1 of the "100% devnet" checklist. **Real remaining
items**: the Raydium CPMM `seed_liquidity_pool` instruction, the 30%
DEX-liquidity vault, `swap-ui/.env`, `RecordPriceSnapshot` vault lockdown,
the full end-to-end devnet test sequence, and the local-validator
integration test suite (see checklist below - none of this is done yet,
only the deploy itself is confirmed).

## MILESTONE: anchor-lang 0.32.1 bump CONFIRMED WORKING - full CI green

Run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/29235257382
Both jobs passed: `cargo check` AND `anchor build` (overall conclusion:
`success`, confirmed via the Actions API directly, not assumed from a
partial log).

This closes out the anchor-lang/anchor-spl 0.32.1 bump work. Final
dependency state in `solana-meme-coin/Cargo.toml`:
- `anchor-lang` / `anchor-spl` = `=0.32.1` (matches Raydium's hard pin)
- `spl-tlv-account-resolution` / `spl-transfer-hook-interface` = `0.9.0`
  (the "Update to Solana v2.1 crates" release - keeps a single consistent
  `solana-program 2.x` generation across the whole graph, avoiding the
  duplicate-generation conflict `0.10.0` introduced)
- `blake3 = "=1.5.5"`, `indexmap = "=2.2.6"`, `zeroize_derive = "=1.4.3"`,
  `unicode-segmentation = "=1.12.0"`: pins working around a known, Anza-
  acknowledged gap between crates.io's ecosystem and `cargo-build-sbf`'s
  currently-bundled Rust toolchain (`1.84.x`, doesn't support `edition2024`
  / the `1.85` MSRV several crates have since adopted). Confirmed via
  Anza's own issue tracker this is the officially endorsed fix pattern,
  not an improvised workaround.

**What this took**: 4 CI round-trips (2 failures caught by `cargo check`
alone in earlier explorations, 3 real `anchor build` failures against
actual Solana BPF toolchain constraints), each one root-caused from a real
error log rather than guessed at - per the defi-blueprint skill's core
rule against inventing library/version details from memory.

**Real next step, now actually unblocked**: devnet deploy
(`anchor deploy --provider.cluster devnet`), then the Raydium CPMM
`seed_liquidity_pool` instruction (the actual remaining coding task).

Run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/29234547838
(`cargo check` passed again; `anchor build` failed further into the build
than last time - real progress each round)

**Real error**: `unicode-segmentation@1.13.3` declares `rust-version =
"1.85.0"` - one above `cargo-build-sbf`'s bundled `1.84.1`. Same class of
issue as the last milestone (bundled BPF toolchain lagging the crates.io
ecosystem), different specific mechanism (an explicit MSRV floor via
Cargo's `rust-version` field, not an edition2024 syntax requirement) - the
error message itself gave the exact fix command, same pattern as before.

**Fix**: tried pinning to `1.13.1` first (the last version without the
1.85 MSRV floor per crates.io's `rust_version` field) - that version is
YANKED, along with `1.13.0`. Fell back to `1.12.0` (last valid, non-yanked
release below the floor). Checked yanked status directly via the sparse
index rather than assuming the pin would resolve.

**Local verification ceiling reached again**: next local error
(`solana-program 2.3.0` needs rustc `1.79`) is a false blocker - CI's real
bundled `1.84.1` already satisfies `1.79`, only the sandbox's `1.75`
doesn't. This is genuinely the end of what local `cargo check` can tell us
- everything past this point needs a real CI run to verify.

Run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/29208466189
(`cargo check` job PASSED this time; `anchor build` job failed on a
`cargo-build-sbf`-specific error - user pulled the real log again)

**Real error**: `zeroize_derive`/`indexmap`/`blake3` (transitive deps)
require Rust's `edition2024`, but `cargo-build-sbf` (Solana's official BPF
build tool) bundles its OWN separate, pinned Rust/Cargo toolchain
(currently `1.84.x`) independent of the system Rust used for the plain
`cargo check` job - that's why `cargo check` passed but `anchor build`
didn't. `edition2024` wasn't stabilized until Rust `1.85`.

**Confirmed via Anza's own GitHub issue tracker (not memory, not a blog)**
this is a known, previously-reported, already-resolved-once issue:
`anza-xyz/agave#8443` (closed as duplicate) -> `anza-xyz/solana-sdk#385`
(closed, state_reason: completed). An Anza maintainer's own fix, quoted
directly: pin the offending transitive crate with
`cargo update --precise <old-ver> -p <crate>@<new-ver>`. This is the
*officially endorsed* fix pattern for this exact class of problem, not an
improvised workaround.

**Real root cause of why THIS project hit it**: traced via the sparse
crates.io index (not guessed) - `anchor-spl 0.32.1` declares a hard
`spl-token-2022 = "^8"` dependency (not a range we can downgrade within).
`spl-token-2022 8.0.1`'s own transitive tree is what pulls in the
edition2024-requiring crates.

**Fix applied, verified locally as far as tooling allows**:
- `spl-tlv-account-resolution` / `spl-transfer-hook-interface`: `0.10.0`
  -> `0.9.0`. Traced via Cargo.lock parsing (not cargo tree, which needed
  a newer rustc than available locally) that `0.10.0` was pulling in a
  SECOND, newer generation of `solana-program` (`4.0.0`) alongside
  anchor-lang's own required `2.x` line - `0.9.0` (the release SPL's own
  changelog describes as "Update to Solana v2.1 crates") resolves to a
  single consistent `solana-program 2.3.0` across the whole graph instead.
- `blake3 = "=1.5.5"`, `indexmap = "=2.2.6"`, `zeroize_derive = "=1.4.3"`:
  each pinned to the newest release that predates its edition2024 move,
  found by iteratively regenerating the lockfile against apt's local
  `rustc 1.75` (old enough to surface the same edition2024 wall as CI's
  bundled `1.84`) and reading the actual "who depends on X" answer
  straight from Cargo.lock's own dependency-edge data each time, rather
  than guessing a version and hoping.

**Honest caveat**: local resolution stopped being useful past this point -
apt's `rustc 1.75` is old enough that it also flags real MSRV gaps
(`borsh-derive 1.7.0` needing `1.77`) that CI's actual `1.84` toolchain
already satisfies. From here, CI is the only trustworthy signal for
whether more edition2024 blockers remain further down the tree.

Run: https://github.com/laithqc-create/solana-meme-coin/actions/runs/29208155045
(user pulled the actual error log, not guessed at)

**Real error**: `spl-pod v0.2.3` (a transitive dependency, pulled in via
the old `spl-token-2022 = "3.0.0"` direct dependency) failed to compile:
`error[E0433]: cannot find decode_error in solana_program` /
`error[E0405]: cannot find trait PrintProgramError`. Root cause: Cargo can
only resolve one version of `solana_program` across the whole dependency
graph. `anchor-lang 0.32.1` pulls in the newer split `solana-program`
crates; the old `spl-pod 0.2.3` (needed by the old `spl-token-2022 3.0.0`)
calls functions that only exist in the pre-split `solana_program` API.
Bumping `anchor-lang` alone was not enough - the SPL token-extension
crates needed bumping too.

**Real fix applied** (verified the code's actual usage first, not just
version-matched blindly):
- Grepped the codebase: `transfer_hook.rs` never imports `spl_token_2022`
  by crate name directly - it goes exclusively through
  `anchor_spl::token_2022::spl_token_2022`, Anchor's own re-export. This
  means the explicit `spl-token-2022 = "3.0.0"` direct dependency in
  Cargo.toml was both unnecessary AND the actual cause of the conflict
  (forcing the old `spl-pod` into the graph). REMOVED it entirely - Anchor
  will pull in whatever `spl-token-2022` version it needs internally.
- `spl-tlv-account-resolution` / `spl-transfer-hook-interface`, which ARE
  used directly by name (for `ExtraAccountMeta`, `ExtraAccountMetaList`,
  `ExecuteInstruction`), bumped `0.6.3` -> `0.10.0` - matched against a
  recent (dated March 2026) real-world guide pairing these exact crate
  versions with the Anchor `0.3x` split-solana-program line.
- Reviewed `transfer_hook.rs`'s actual API usage
  (`ExtraAccountMeta::new_with_pubkey`, `ExtraAccountMetaList::init`,
  `StateWithExtensions::unpack`, `get_extension::<TransferHookAccount>`)
  before pushing - these are foundational, long-stable APIs in this
  interface, reasonable bet they still hold at 0.10.0, but NOT compiler-
  verified yet.

**Still not verified** - this is the next CI run to check, not a done
deal. If it fails again, get the real log again rather than guessing at
a third fix blind.

**Confirmed target version via primary source**: fetched Raydium's actual
`raydium-io/raydium-cpi` README directly (not memory) - it states their CPI
adapters deliberately lag Anchor's newest releases for AMM contract
stability, and their most current supported line requires exactly
`anchor-lang = "=0.32.1"`, `anchor-cli 0.32.1`, `solana-cli 2.3.0` - a hard
pin, not a minimum. Anchor itself has since shipped a stable `1.0.x` line
(confirmed via their own release notes), which is NOT what Raydium
supports yet - bumping this program to `1.0.x` instead would have broken
compatibility with the Raydium CPMM CPI crate. Confirmed `0.32.1` is the
correct target, not `1.0.x`.

**This also explains the earlier "Not in a Solana workspace" workflow
failure**: unpinned `avm install latest` grabs Anchor's newest CLI
(now `1.0.x`, which recommends Solana CLI `3.1.10` - the version number
seen in that failure log), not a real bug in workspace layout.

**Applied to `solana-meme-coin/Cargo.toml`**:
- `anchor-lang`/`anchor-spl`: `0.30.1` -> `=0.32.1` (exact pin, matching
  Raydium's own pin, to avoid the resolver picking a different patch
  version than what their CPI crate expects)
- Removed the direct `solana-program = "1.18.17"` dependency entirely -
  Anchor's own docs recommend this since 0.31+ to avoid conflicts between
  the old monolithic solana-program and the new split v2.x crates that
  anchor-lang 0.32.1 pulls in (`solana-account-info`, `solana-clock`,
  `solana-cpi`, etc., all pinned to `"2"`). Checked first: no code in this
  repo does `use solana_program::...` directly (grepped, zero hits outside
  the `anchor_lang::solana_program` re-export) - so this removal has no
  code-level fallout to fix.

**Also fixed the same unpinned-`avm install latest` bug in the REAL CI**
(`rust-check.yml`'s `anchor-build` job, not just the keypair-generation
workflow) - would have hit the identical failure mode on the very next
push otherwise, and any resulting failure would have looked like a code
problem rather than a tooling-pin problem. Pinned to `avm install 0.32.1`
/ `avm use 0.32.1` there too.

**Verification status - be honest about what's actually been checked**:
- Confirmed via a real `cargo check` attempt (installed `rustc`/`cargo`
  1.75.0 from Ubuntu's apt repo in the sandbox, since rustup's installer
  domain isn't reachable there) that the Cargo dependency GRAPH resolves
  cleanly - `anchor-lang 0.32.1` + `anchor-spl 0.32.1` + the existing SPL
  crate versions have no version conflicts.
- Did NOT get a real compile check locally - Ubuntu's `rustc 1.75.0` is
  too old (a transitive dependency, `blake3`, requires Rust's
  `edition2024`, needing `rustc 1.85+`), and no newer toolchain was
  reachable from this sandbox's network allowlist.
- **The actual code-level compile check (does `vesting.rs` /
  `transfer_hook.rs` / `presale.rs` / `curve.rs` still compile against the
  bumped APIs) has NOT happened yet** - this push to CI is that real test.
  Don't treat this bump as "verified" until CI reports back.

## MILESTONE: Real program ID landed, but anchor keys sync only partially applied - fixed

Workflow ran (twice, per user). Confirmed commit `24713bb` on `claude-session-fixes`:
"Sync real program ID (EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N) via
anchor keys sync". `lib.rs`'s `declare_id!` is correctly updated to this ID.

**Bug found and fixed**: `anchor keys sync` only updated
`Anchor.toml`'s `[programs.localnet]` entry to the real ID - it left
`[programs.devnet]` on the placeholder `TokenVesting1111...`. Since actual
devnet deployment reads the `devnet` entry specifically, this would have
silently deployed against/referenced a fake program ID. Fixed by hand
(one-line edit, `[programs.devnet]` now also
`EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N`) - not yet pushed, needs a
PAT or manual push (see blockers).

**RESOLVED**: user confirmed the second run failed outright at the `anchor
keys sync` step (log: "Not in a Solana workspace... Error: Process
completed with exit code 1"), BEFORE the commit/artifact-upload steps ever
ran - so no orphaned keypair/artifact exists from that run. The first run
is the only real one and it's the one already committed
(`EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N`).

**Real bug found in the workflow itself, now fixed**: it ran `avm install
latest` / `avm use latest` - not pinned. The two runs almost certainly
grabbed two different Anchor CLI versions; the failing run's log showed
`3.1.10`, a major-version jump past the `0.3x` line this whole project's
`anchor-lang` dependency is built against, and its workspace-detection
logic apparently no longer recognized this repo's `Anchor.toml` layout.
Fixed by pinning `avm install 0.32.1` / `avm use 0.32.1` explicitly
(matches the anchor-lang version this project is bumping to for the
Raydium CPMM CPI crate - see milestone above) - NOT yet pushed, needs a
PAT or manual push.

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

### Rust program - CODE COMPLETE AND CI-VERIFIED (anchor build passes)
- `vesting.rs`, `transfer_hook.rs`, `presale.rs`, `curve.rs`, `errors.rs`
  all in place, `cargo check` + real `anchor build` (Solana BPF toolchain)
  both green as of run 29235257382.
- `anchor-lang`/`anchor-spl` on `=0.32.1` (Raydium CPMM CPI requirement) -
  see milestones above for the full dependency-conflict chain this took
  to resolve.

### Program identity - REAL, not placeholder
- Program ID: `EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N`, synced
  everywhere (`lib.rs` declare_id!, Anchor.toml devnet + localnet).
- Private keypair generated via the `generate-program-keypair.yml`
  workflow, downloaded by user - confirm it's in real secure storage, not
  just a Downloads folder, since this is the deploy/upgrade authority key.

### CI (GitHub Actions) - fully working
- `.github/workflows/rust-check.yml`: `cargo-check` + `anchor-build` both
  passing on every push to `claude-session-fixes`.

### swap-ui - DONE, fully verified
- Real Phantom/Solflare wallet connection, real swap execution via
  Jupiter's `/v6/swap-instructions`, Token-2022 fee-instruction injection
  matching the transfer hook's sibling-instruction requirement.
- VERIFIED: `npm install`, `tsc --noEmit`, `vitest run` (2/2 pass), `vite
  build` all clean.
- NOT wired: `OtcPortal.tsx` still fully mocked - explicitly OUT of the
  "100% devnet" scope per user, see Project phasing below.

## Project phasing (user-confirmed)
**Phase A (current focus): get THIS project to 100% verified on devnet**
before starting anything new. **Phase B (next, after Phase A is fully
done): "launchpad curve"** - scope not yet defined in this file, need to
clarify with user what this covers (a generalized/reusable version of the
bonding-curve mechanism for multiple token launches? a separate product?)
before Phase B work starts. Do not start Phase B work until Phase A's
checklist below is fully checked off AND Phase B's scope is confirmed.

## Current file being worked on
- Nothing in progress - devnet deploy is DONE and verified (see milestone
  above). Next action is the `seed_liquidity_pool` instruction (Phase A,
  item 2 below).

## Exact next steps (in order) - this IS the "100% devnet" checklist
1. ~~**Devnet deploy**~~ - DONE, confirmed via live `solana program show`
   query against devnet's own RPC (see milestone above for full output).
   Program `EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N` is real,
   upgradeable, and live on devnet as of slot 476186173.
2. **Build the `seed_liquidity_pool` instruction** using `raydium-cpmm-cpi`
   (git: `raydium-io/raydium-cpi`, `cpi` feature, `anchor-lang = "=0.32.1"`
   already matches what's pinned in this repo) - gated on
   `presale_state.sold_out == true`, CPIs the collected SOL out of the
   `presale_treasury` PDA (as signer, via `invoke_signed`) plus the 30%
   DEX-liquidity token allocation into a new Raydium CPMM pool
   (`InitializeCpmm`).
3. **The 30% DEX-liquidity token allocation still isn't custodied
   anywhere** - needs its own instruction (likely at `initialize_presale`
   time) transferring 30% of total supply into a PDA-owned vault analogous
   to `presale_treasury`, before item 2 has tokens to deposit alongside
   the SOL.
4. **Fill in `swap-ui/.env`** with the real deployed mint address,
   marketing wallet address, decimals, and devnet RPC URL.
5. **Hardcode real pool vault pubkeys** into `RecordPriceSnapshot`'s
   account constraints in `vesting.rs` - currently accepts ANY two token
   accounts. Blocked until item 2 (liquidity actually deployed to a pool).
6. **Full end-to-end test on devnet**: initialize_presale -> buy_tokens
   (repeatedly, exercising the curve's full range, including the exact-
   sellout capping case) -> activate_tge -> claim_tge ->
   initialize_vesting (client-side, 90% of allocation) ->
   finalize_investor_vesting -> wait past cliff -> release_vesting ->
   simulate a 30%+ price drop via record_price_snapshot +
   check_and_trigger_volatility_delay -> confirm the 7-day delay applies ->
   confirm the NEXT month's release is still on the original calendar grid
   (not shifted). This is the actual "100%" bar per the defi-blueprint
   skill's three-tier testing pipeline (in-memory unit tests are already
   done; local-validator `anchor test` integration suite for CPI/compute-
   budget behavior has NOT been run yet and should happen before/alongside
   this devnet pass, not skipped).

## Explicitly OUT of "100% devnet" scope (separate, later work)
- `OtcPortal.tsx` / spec section 7's on-chain OTC contract - not started,
  not scoped.
- Pre-mainnet housekeeping (professional security audit of `curve.rs`
  specifically, real securities lawyer review) - needed before mainnet,
  not before devnet completion.
- Phase B "launchpad curve" - scope TBD, starts only after Phase A is
  fully checked off above.

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
2. AMM choice is RESOLVED: Raydium CPMM. anchor-lang/anchor-spl bump to
   0.32.1 is RESOLVED and CI-CONFIRMED (run 29235257382, both jobs green).
   Do not re-litigate either unless something material changes.
3. Program keypair is real and synced: `EkF67nLhbAzj45Sv3ggYRLq5NLUXrp1bLei2h4APGJ3N`
   (both lib.rs declare_id! and Anchor.toml's devnet + localnet entries).
4. **Devnet deploy is DONE and CONFIRMED** via a live `solana program show`
   query against devnet's own RPC - not just a self-reported success.
   Program is real, upgradeable, deployed at slot 476186173. Deployed via
   `.github/workflows/deploy-devnet.yml`, which uses a PERSISTENT payer
   wallet (`DEVNET_PAYER_KEYPAIR` secret, funded via the web faucet -
   reusable for future redeploys without fighting devnet's CLI-airdrop
   IP rate-limiting again).
5. Next real action: build the `seed_liquidity_pool` instruction using
   `raydium-cpmm-cpi` (git dependency on raydium-io/raydium-cpi,
   `anchor-lang = "=0.32.1"` matches what's already pinned here), gated on
   `presale_state.sold_out`. This also needs the 30% DEX-liquidity vault
   instruction (nothing custodies that allocation yet) built alongside/
   before it - see checklist items 2-3 above.
6. Separate open decision, still pending: merge `claude-session-fixes` to
   `main` now vs. keep iterating on the branch. Not blocking - revisit
   whenever convenient.
