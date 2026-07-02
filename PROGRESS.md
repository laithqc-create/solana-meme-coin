# Project Progress

## Compliance gate (resolved this session)
- Automated volume-generation / wash-trading bot from the original spec doc:
  REJECTED as a ToS/market-manipulation violation. Dropped from scope permanently.
- Jurisdiction: user confirmed global public presale, no KYC/geofencing, anonymous
  team, mint+admin authority destroyed. User explicitly acknowledged in writing
  ("I understand and accept the legal risk of an anonymous, unrestricted global
  presale") the securities-law risk this creates (Howey-pattern: public sale +
  discount pricing + vesting tied to team effort + no buyer verification). Not
  blocked on my end per the skill's rules — this is the user's legal risk to own,
  not mine to gate — but it must stay visible in this file so it's never silently
  forgotten in a later session. Recommend a real securities lawyer review before
  mainnet / real funds move.

## What's been completed
- Architecture plan approved (Phase 2 blueprint checklist confirmed by user).
- Workspace setup initiated (Cargo project created).
- Web UI (Phase 3, prior session) fully built:
  - Vite + React + TypeScript, Jupiter swap widget, OTC portal, wallet adapter,
    Vitest scaffolded with first passing test.
- Smart contract — THIS SESSION'S CHANGES:
  - `vesting.rs` — REWRITTEN. Was a stub (empty `release_vesting`, fake
    `trigger_volatility_delay`). Now has:
    - Real cliff (30d) + 8%/month release math, capped at 13 tranches to 100%.
    - `record_price_snapshot` — permissionless keeper-crank instruction reading
      AMM pool vault token-account balances directly (no third-party pool SDK
      dependency), stores into a 48-slot ring buffer (~1 snapshot/hour).
    - `check_and_trigger_volatility_delay` — permissionless, compares latest
      snapshot vs. oldest snapshot inside the 48h window; on >=30% drop, delays
      ONLY the currently-maturing tranche by up to 7 days, without shifting the
      fixed timestamp grid for any future month (matches spec section 4 exactly).
    - `release_vesting` now actually transfers tokens via `transfer_checked` CPI,
      signed by the vesting PDA.
    - Unit tests included for cliff math, month-index math, 8%/tranche math,
      30% vs 29% drop threshold, and the "delay doesn't shift future grid" invariant.
  - `transfer_hook.rs` — REWRITTEN with an important architecture correction:
    - Original stub implied the hook could CPI the 1% tax directly to the
      marketing wallet. THIS IS NOT POSSIBLE — Token-2022 blocks any nested
      transfer of the same mint from inside its own hook (reentrancy guard).
    - Real implementation: sibling-instruction enforcement. The hook inspects
      the Instructions sysvar for a companion TransferChecked instruction (same
      tx) paying >=1% of the trade amount to the marketing wallet, and REJECTS
      the whole transaction if it's missing when source/destination is the
      known DEX pool. P2P transfers (neither side is the pool) pass with no
      fee requirement.
    - Consequence for the swap-ui: the Jupiter-integrated swap widget MUST be
      updated to attach this sibling fee-payment instruction whenever the route
      touches the pool, or every DEX trade will fail at the hook. NOT YET DONE
      in swap-ui — this is the next concrete task.
  - `lib.rs` updated: `trigger_volatility_delay` instruction replaced with the
    two new permissionless crank instructions (`record_price_snapshot`,
    `check_and_trigger_volatility_delay`).
  - `Cargo.toml` updated: anchor-spl features `["token_2022", "token_2022_extensions"]`
    added (required for `token_interface`, `TransferHookAccount`, extension helpers).

## Current file being worked on
- Just finished `vesting.rs` and `transfer_hook.rs` rewrites.

## Exact next steps
1. **No Rust toolchain in the sandbox this was written in — NOT YET COMPILed.**
   Run `cargo check` / `anchor build` locally or via your GitHub Action before
   trusting any of this. Flag back with the exact error if it doesn't build —
   don't just re-guess blind.
2. `RecordPriceSnapshot` accounts currently accept ANY two token accounts as
   pool vaults with no on-chain check that they're the canonical pool. Before
   mainnet, hardcode/verify the real pool vault pubkeys (constraint left as a
   TODO comment in the file) — otherwise anyone could feed fake reserves and
   manipulate the volatility trigger.
3. DONE this session — `swap-ui` full swap execution + fee-instruction wiring:
   - Found the widget was previously quote-only (no execution, no wallet
     adapters registered, "Swap Tokens" button had no onClick). Flagged to
     user, confirmed scope, then built the real thing:
   - `package.json`: added `@solana/spl-token`, `@solana/wallet-adapter-wallets`.
   - `App.tsx`: registered Phantom + Solflare wallet adapters (was `[]`),
     added `WalletMultiButton` connect UI.
   - New file `src/lib/swapTransaction.ts`:
     - `buildTaxedSwapTransaction()` — calls Jupiter's `/v6/swap-instructions`
       (raw instructions, not a pre-built tx) so the fee instruction can be
       spliced in before compiling. Builds a Token-2022 `transferChecked`
       instruction paying the marketing wallet's MEME ATA, sized per
       `computeRequiredFee()`:
       - SELL side (MEME->SOL): exact fee, input amount is known precisely.
       - BUY side (SOL->MEME): fee based on quoted `outAmount` + 0.1% buffer,
         since actual output isn't known until execution — flagged as an
         approximation to tighten via simulation before mainnet (see comment
         in file).
     - Resolves address lookup tables, compiles a v0 `VersionedTransaction`
       with compute-budget + setup + fee + swap + cleanup instructions.
   - `SwapWidget.tsx` rewritten: real `useWallet`/`useConnection` hooks,
     builds+signs+sends+confirms the taxed swap tx, shows tx status and a
     Solscan link on success.
   - VERIFIED (this sandbox has Node, unlike Rust): `npm install`,
     `tsc --noEmit` clean, `vitest run` — 2/2 existing tests still pass,
     `vite build` — clean production build. This is real verification, not
     a guess — same confidence level as the Rust side does NOT have yet.
   - NOT wired: `OtcPortal.tsx` is still fully mocked (fake pool stats, no
     on-chain calls) — spec section 7's actual OTC swap contract doesn't
     exist yet, in Rust or the UI. Not touched this session.
   - Config needed before this actually works end-to-end: `.env` needs
     `VITE_MEME_COIN_MINT_ADDRESS`, `VITE_MARKETING_WALLET_ADDRESS`,
     `VITE_MEME_DECIMALS`, `VITE_SOLANA_RPC_URL` — none of these exist yet
     since the token isn't deployed.
4. FIXED this session: `presale.rs` migrated from legacy `anchor_spl::token`
   to `anchor_spl::token_interface` (Token-2022 compatible) throughout —
   `presale_vault`, `buyer_token_account`, `mint` all now use
   `InterfaceAccount`/`Interface` types, `transfer_checked` replaces the old
   `token::transfer`.
   Added `finalize_investor_vesting` instruction + `FinalizeInvestorVesting`
   accounts struct wiring `presale.rs` to `vesting.rs`:
     - `BuyerState` gained a `vesting_funded: bool` field (space updated
       8+8+8+1 -> +1 more byte).
     - Flow is now: `buy_tokens` -> `claim_tge` (10%) -> client calls
       `vesting::initialize_vesting` directly (beneficiary=buyer, mint,
       total_amount = total_allocation - claimed_amount) -> client calls
       `presale::finalize_investor_vesting`, which cross-checks the vesting
       account's beneficiary/mint/total_amount against buyer_state and, only
       if they match exactly, transfers the 90% from `presale_vault` into the
       vesting PDA's token account. This is intentionally two client calls
       rather than one CPI-from-CPI, to keep `initialize_vesting`'s account
       validation (PDA derivation, mint checks) in the caller's control
       rather than trying to nest Anchor instruction dispatch inside itself.
     - FIXED the missing piece noted above: added `activate_tge` instruction
       (admin-only, checked via a new `admin: Pubkey` field on `PresaleState`
       set at `initialize_presale` and enforced with `has_one` on
       `ActivateTGE`). `claim_tge` will now actually work once admin calls it.
5. After local compile passes: devnet deploy, wire program IDs into swap-ui,
   end-to-end test presale -> TGE claim -> cliff -> first monthly release ->
   simulated 30% price drop -> confirm 7-day delay -> confirm next month's
   grid position is unaffected.

## Any blockers or decisions pending
- Waiting on local/CI compile results (item 1 above).
- Real pool vault addresses not yet known (item 2) — presumably available once
  the launchpad curve actually deploys liquidity to the pool.

---
### RESUME FROM HERE
- Rust side (program): still UNCOMPILED — no toolchain in this sandbox.
  Get `cargo check`/`anchor build` output from your machine or the GitHub
  Action and paste it back. Highest-priority thing to verify first.
- TypeScript side (swap-ui): VERIFIED — typecheck, tests, and prod build
  all pass clean as of this session. Confidence here is real, not a guess.
- Once the program compiles and deploys to devnet: fill in swap-ui's `.env`
  with the real mint/marketing-wallet addresses, and hardcode the real pool
  vault pubkeys into `RecordPriceSnapshot`'s account constraints (currently
  accepts any two token accounts — see item 2 above).
- `presale.rs`'s `buy_tokens` uses plain `+=`/`*` arithmetic with no
  overflow checks — pre-existing from before this session, not touched, but
  worth a pass before mainnet (Anchor's release profile does NOT panic on
  overflow by default the way debug does).
