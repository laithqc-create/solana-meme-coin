import {
  AddressLookupTableAccount,
  Connection,
  PublicKey,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from '@solana/web3.js';
import {
  TOKEN_2022_PROGRAM_ID,
  createTransferCheckedInstruction,
  getAssociatedTokenAddressSync,
} from '@solana/spl-token';

// ---- Config, all overridable via .env ----
export const MEME_MINT = new PublicKey(
  import.meta.env.VITE_MEME_COIN_MINT_ADDRESS || 'TokenVesting1111111111111111111111111111111'
);
export const MEME_DECIMALS = Number(import.meta.env.VITE_MEME_DECIMALS || 9);
export const MARKETING_WALLET = new PublicKey(
  import.meta.env.VITE_MARKETING_WALLET_ADDRESS || '11111111111111111111111111111111111111111'
);
// 1% tax, matches TAX_BPS in transfer_hook.rs — keep these two in sync manually.
export const TAX_BPS = 100n;
export const TAX_BPS_DENOMINATOR = 10_000n;
// Small buffer added to the fee floor on the "buy" (SOL->MEME) direction only,
// where the actual output amount isn't known exactly ahead of execution (see
// buildTaxedSwapTransaction doc comment below).
const BUY_SIDE_FEE_BUFFER_BPS = 10n; // +0.1%

interface JupiterRawInstruction {
  programId: string;
  accounts: { pubkey: string; isSigner: boolean; isWritable: boolean }[];
  data: string; // base64
}

interface SwapInstructionsResponse {
  tokenLedgerInstruction?: JupiterRawInstruction;
  computeBudgetInstructions: JupiterRawInstruction[];
  setupInstructions: JupiterRawInstruction[];
  swapInstruction: JupiterRawInstruction;
  cleanupInstruction?: JupiterRawInstruction;
  addressLookupTableAddresses: string[];
}

function toTransactionInstruction(ix: JupiterRawInstruction): TransactionInstruction {
  return new TransactionInstruction({
    programId: new PublicKey(ix.programId),
    keys: ix.accounts.map((a) => ({
      pubkey: new PublicKey(a.pubkey),
      isSigner: a.isSigner,
      isWritable: a.isWritable,
    })),
    data: Buffer.from(ix.data, 'base64'),
  });
}

/**
 * Computes the exact 1% tax the transfer_hook program will require for this
 * trade, and returns the TransactionInstruction that pays it — this MUST be
 * included as a sibling instruction in the same transaction as the swap, or
 * transfer_hook.rs's verify_sibling_fee_instruction will reject the whole
 * transaction (see transfer_hook.rs for why the hook can't just take the fee
 * itself: Token-2022 blocks nested transfers of the same mint from inside
 * its own hook).
 *
 * Fee-basis note:
 *  - SELL (MEME -> SOL/USDC): the amount of MEME leaving the user's wallet
 *    is the exact, known `amountLamports` the user typed in — no slippage
 *    uncertainty on this side, so the fee is exact.
 *  - BUY (SOL/USDC -> MEME): the exact MEME amount the user will receive
 *    isn't known until the swap executes on-chain; only the Jupiter quote's
 *    estimate is known ahead of time. We pay tax on the quoted `outAmount`
 *    plus a small buffer so genuinely tiny amounts of positive slippage
 *    don't cause the hook to reject the transaction. If actual output ever
 *    comes in higher than quote+buffer, the transaction would revert at the
 *    hook — before mainnet this should be tightened by simulating the swap
 *    first and reading the real output amount rather than trusting the quote.
 */
function computeRequiredFee(
  direction: 'buy' | 'sell',
  amountLamports: bigint,
  quoteOutAmount: bigint
): bigint {
  if (direction === 'sell') {
    return (amountLamports * TAX_BPS) / TAX_BPS_DENOMINATOR;
  }
  const buffered = (quoteOutAmount * (TAX_BPS_DENOMINATOR + BUY_SIDE_FEE_BUFFER_BPS)) / TAX_BPS_DENOMINATOR;
  return (buffered * TAX_BPS) / TAX_BPS_DENOMINATOR;
}

export interface BuildSwapArgs {
  connection: Connection;
  userPublicKey: PublicKey;
  quoteResponse: any; // raw response object from Jupiter /v6/quote
  inputMint: string;
  outputMint: string;
  amountLamports: bigint; // exact-in amount, in the input mint's base units
}

export async function buildTaxedSwapTransaction({
  connection,
  userPublicKey,
  quoteResponse,
  inputMint,
  outputMint,
  amountLamports,
}: BuildSwapArgs): Promise<VersionedTransaction> {
  const memeMintStr = MEME_MINT.toBase58();
  const touchesMeme = inputMint === memeMintStr || outputMint === memeMintStr;

  if (!touchesMeme) {
    throw new Error('This swap does not involve the project token — nothing to tax-wire here.');
  }

  const direction: 'buy' | 'sell' = outputMint === memeMintStr ? 'buy' : 'sell';
  const quoteOutAmount = BigInt(quoteResponse.outAmount);
  const requiredFee = computeRequiredFee(direction, amountLamports, quoteOutAmount);

  // 1. Get the raw instruction set from Jupiter (not a pre-built transaction,
  //    so we can splice in the fee instruction before compiling).
  const res = await fetch('https://quote-api.jup.ag/v6/swap-instructions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      quoteResponse,
      userPublicKey: userPublicKey.toBase58(),
      wrapAndUnwrapSol: true,
    }),
  });
  if (!res.ok) {
    throw new Error(`Jupiter swap-instructions request failed: ${res.status}`);
  }
  const swapIxs: SwapInstructionsResponse = await res.json();

  const computeBudgetIxs = swapIxs.computeBudgetInstructions.map(toTransactionInstruction);
  const setupIxs = swapIxs.setupInstructions.map(toTransactionInstruction);
  const swapIx = toTransactionInstruction(swapIxs.swapInstruction);
  const cleanupIxs = swapIxs.cleanupInstruction
    ? [toTransactionInstruction(swapIxs.cleanupInstruction)]
    : [];

  // 2. Build the fee-payment instruction the transfer hook requires.
  const userMemeAta = getAssociatedTokenAddressSync(
    MEME_MINT,
    userPublicKey,
    false,
    TOKEN_2022_PROGRAM_ID
  );
  const marketingMemeAta = getAssociatedTokenAddressSync(
    MEME_MINT,
    MARKETING_WALLET,
    false,
    TOKEN_2022_PROGRAM_ID
  );
  const feeIx = createTransferCheckedInstruction(
    userMemeAta,
    MEME_MINT,
    marketingMemeAta,
    userPublicKey,
    requiredFee,
    MEME_DECIMALS,
    [],
    TOKEN_2022_PROGRAM_ID
  );

  // 3. Resolve address lookup tables Jupiter's route depends on.
  const lookupTableAccounts: AddressLookupTableAccount[] = [];
  for (const addr of swapIxs.addressLookupTableAddresses) {
    const lut = await connection.getAddressLookupTable(new PublicKey(addr));
    if (lut.value) lookupTableAccounts.push(lut.value);
  }

  // 4. Compile. Fee instruction placed after setup, before the swap itself —
  //    position doesn't matter to the hook (it scans every sibling
  //    instruction in the tx regardless of order), this ordering is just
  //    for readability in explorers.
  const { blockhash } = await connection.getLatestBlockhash();
  const message = new TransactionMessage({
    payerKey: userPublicKey,
    recentBlockhash: blockhash,
    instructions: [...computeBudgetIxs, ...setupIxs, feeIx, swapIx, ...cleanupIxs],
  }).compileToV0Message(lookupTableAccounts);

  return new VersionedTransaction(message);
}
