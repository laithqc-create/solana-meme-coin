/**
 * One-off devnet bootstrap script - NOT part of the on-chain program or
 * swap-ui. Run once, after the mint itself has already been created via
 * `spl-token create-token --transfer-hook <PROGRAM_ID> ...` (see
 * .github/workflows/setup-devnet-mint.yml) and the full 1B supply has
 * already been minted to the payer's own ATA.
 *
 * This script:
 *   1. Calls initialize_presale (creates presale_state, presale_vault,
 *      dex_liquidity_vault, pool_creator_authority).
 *   2. Calls initialize_extra_account_meta_list with a PLACEHOLDER
 *      dex_pool (the payer's own wallet - deliberately obviously not a
 *      real pool address) and a placeholder marketing_wallet (also the
 *      payer's wallet for devnet purposes). See PROGRESS.md's
 *      "update_dex_pool" milestone for why a placeholder is unavoidable
 *      at this point in the flow, and why update_dex_pool MUST be called
 *      later once the real Raydium pool exists.
 *   3. Transfers exactly 300,000,000 tokens (30% of the 1B supply) to
 *      presale_vault, and another 300,000,000 to dex_liquidity_vault,
 *      using transferCheckedWithTransferHook so the transfer-hook's
 *      extra accounts get resolved automatically.
 *
 * Required environment variables:
 *   RPC_URL            - devnet RPC endpoint
 *   PAYER_KEYPAIR_PATH - path to the funded payer wallet's keypair JSON
 *   PROGRAM_ID         - this program's deployed address
 *   MINT_ADDRESS       - the Token-2022 mint created in the prior step
 *   IDL_PATH           - path to the anchor-build-generated IDL JSON
 */

import * as fs from "fs";
import * as anchor from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  transferCheckedWithTransferHook,
} from "@solana/spl-token";

const DECIMALS = 9;
const VAULT_ALLOCATION = 300_000_000n * 10n ** BigInt(DECIMALS); // 300M tokens, 30% each

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

function loadKeypair(path: string): Keypair {
  const raw = JSON.parse(fs.readFileSync(path, "utf-8"));
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

async function main() {
  const rpcUrl = requireEnv("RPC_URL");
  const payerPath = requireEnv("PAYER_KEYPAIR_PATH");
  const programId = new PublicKey(requireEnv("PROGRAM_ID"));
  const mint = new PublicKey(requireEnv("MINT_ADDRESS"));
  const idlPath = requireEnv("IDL_PATH");

  const connection = new Connection(rpcUrl, "confirmed");
  const payer = loadKeypair(payerPath);

  const wallet = new anchor.Wallet(payer);
  const provider = new anchor.AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);

  const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));
  const program = new anchor.Program(idl, provider);

  console.log("Payer / admin:", payer.publicKey.toBase58());
  console.log("Mint:", mint.toBase58());
  console.log("Program ID:", programId.toBase58());

  // --- Derive every PDA using the exact same seeds as the Rust program ---
  const [presaleState] = PublicKey.findProgramAddressSync(
    [Buffer.from("presale_state")],
    programId
  );
  const [presaleVault] = PublicKey.findProgramAddressSync(
    [Buffer.from("presale_vault")],
    programId
  );
  const [dexLiquidityVault] = PublicKey.findProgramAddressSync(
    [Buffer.from("dex_liquidity_vault")],
    programId
  );
  const [poolCreatorAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool_creator_authority")],
    programId
  );
  const [extraAccountMetaList] = PublicKey.findProgramAddressSync(
    [Buffer.from("extra-account-metas"), mint.toBuffer()],
    programId
  );

  console.log("presale_state:", presaleState.toBase58());
  console.log("presale_vault:", presaleVault.toBase58());
  console.log("dex_liquidity_vault:", dexLiquidityVault.toBase58());
  console.log("pool_creator_authority:", poolCreatorAuthority.toBase58());
  console.log("extra_account_meta_list:", extraAccountMetaList.toBase58());

  // --- Guard against devnet RPC propagation lag ---
  // api.devnet.solana.com is a load-balanced, multi-node public endpoint -
  // a mint confirmed by one backend node isn't guaranteed to be instantly
  // visible to whichever node handles the NEXT request, even a few
  // seconds later. Poll until this specific connection can actually see
  // it before submitting a transaction that depends on it existing.
  console.log("\nWaiting for mint account to be visible to this RPC connection...");
  const mintWaitStart = Date.now();
  const mintWaitTimeoutMs = 60_000;
  while (true) {
    const info = await connection.getAccountInfo(mint, "confirmed");
    if (info !== null) {
      console.log(`  Mint visible after ${Date.now() - mintWaitStart}ms.`);
      break;
    }
    if (Date.now() - mintWaitStart > mintWaitTimeoutMs) {
      throw new Error(
        `Mint account ${mint.toBase58()} still not visible to this RPC connection after ${mintWaitTimeoutMs}ms - this points to something more than ordinary propagation lag.`
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 3000));
  }

  // --- Step 1: initialize_presale ---
  console.log("\n[1/3] Calling initialize_presale...");
  const existingPresale = await connection.getAccountInfo(presaleState);
  if (existingPresale) {
    console.log("  presale_state already exists - skipping (idempotent re-run).");
  } else {
    const sig = await program.methods
      .initializePresale()
      .accounts({
        presaleState,
        presaleVault,
        dexLiquidityVault,
        poolCreatorAuthority,
        mint,
        admin: payer.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([payer])
      .rpc();
    console.log("  initialize_presale tx:", sig);
  }

  // --- Step 2: initialize_extra_account_meta_list (placeholder dex_pool) ---
  console.log("\n[2/3] Calling initialize_extra_account_meta_list...");
  const existingMetaList = await connection.getAccountInfo(extraAccountMetaList);
  if (existingMetaList) {
    console.log("  extra_account_meta_list already exists - skipping (idempotent re-run).");
    console.log(
      "  REMINDER: its dex_pool value is almost certainly still the placeholder - call update_dex_pool once the real pool exists."
    );
  } else {
    const sig = await program.methods
      .initializeExtraAccountMetaList()
      .accounts({
        payer: payer.publicKey,
        presaleState,
        extraAccountMetaList,
        mint,
        dexPool: payer.publicKey, // PLACEHOLDER - see module doc comment above
        marketingWallet: payer.publicKey, // PLACEHOLDER - real marketing wallet not yet decided (see PROGRESS.md)
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([payer])
      .rpc();
    console.log("  initialize_extra_account_meta_list tx:", sig);
    console.log(
      "  REMINDER: dex_pool and marketing_wallet are BOTH placeholders (payer's own wallet). Call update_dex_pool once the real pool exists."
    );
  }

  // --- Step 3: fund both vaults with their 300M allocations ---
  const payerAta = getAssociatedTokenAddressSync(
    mint,
    payer.publicKey,
    false,
    TOKEN_2022_PROGRAM_ID
  );

  console.log("\n[3/3] Funding presale_vault and dex_liquidity_vault (300M each)...");
  for (const [label, destination] of [
    ["presale_vault", presaleVault],
    ["dex_liquidity_vault", dexLiquidityVault],
  ] as const) {
    const destInfo = await connection.getAccountInfo(destination);
    // A freshly-created SPL token account's data is all zeros except for
    // mint/owner - amount (a u64 at a fixed offset) being non-zero is
    // enough to treat this as "already funded" for idempotent re-runs.
    const alreadyFunded =
      destInfo && destInfo.data.length >= 72 && destInfo.data.readBigUInt64LE(64) > 0n;
    if (alreadyFunded) {
      console.log(`  ${label} already funded - skipping (idempotent re-run).`);
      continue;
    }
    const sig = await transferCheckedWithTransferHook(
      connection,
      payer,
      payerAta,
      mint,
      destination,
      payer,
      VAULT_ALLOCATION,
      DECIMALS,
      [],
      { commitment: "confirmed" },
      TOKEN_2022_PROGRAM_ID
    );
    console.log(`  ${label} funded, tx:`, sig);
  }

  console.log("\nDevnet setup complete.");
  console.log(
    "IMPORTANT: dex_pool and marketing_wallet in extra_account_meta_list are placeholders (payer's own wallet)."
  );
  console.log(
    "Once seed_liquidity_pool has run and a real pool exists, call update_dex_pool with the real token vault address before relying on the 1% tax."
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
