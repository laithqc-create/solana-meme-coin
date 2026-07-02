import {
    Connection,
    Keypair,
    SystemProgram,
    Transaction,
    sendAndConfirmTransaction,
} from '@solana/web3.js';
import {
    ExtensionType,
    createInitializeMintInstruction,
    getMintLen,
    TOKEN_2022_PROGRAM_ID,
    createInitializeTransferHookInstruction,
    createMintToInstruction,
    createSetAuthorityInstruction,
    AuthorityType,
} from '@solana/spl-token';

// IMPORTANT: This script uses the Token-2022 program to create a mint with a Transfer Hook.
// Ensure your connection is pointed to the correct cluster (devnet/mainnet).

export async function initializeMemeCoinMint(
    connection: Connection,
    payer: Keypair,
    transferHookProgramId: string
) {
    // 1. Generate a new keypair for the Mint
    const mintKeypair = Keypair.generate();
    const decimals = 9;
    const totalSupply = 1_000_000_000 * 10 ** decimals; // 1 Billion tokens

    // Calculate space required for the mint with Transfer Hook extension
    const extensions = [ExtensionType.TransferHook];
    const mintLen = getMintLen(extensions);
    const lamports = await connection.getMinimumBalanceForRentExemption(mintLen);

    console.log(`Creating Meme Coin Mint: ${mintKeypair.publicKey.toBase58()}`);

    // 2. Build the initialization transaction
    const transaction = new Transaction().add(
        // Create the account for the Mint
        SystemProgram.createAccount({
            fromPubkey: payer.publicKey,
            newAccountPubkey: mintKeypair.publicKey,
            space: mintLen,
            lamports,
            programId: TOKEN_2022_PROGRAM_ID,
        }),
        // Initialize the Transfer Hook Extension FIRST
        createInitializeTransferHookInstruction(
            mintKeypair.publicKey,
            payer.publicKey, // Authority that can update the hook later if needed, or null to lock
            transferHookProgramId, // Our custom Rust program ID
            TOKEN_2022_PROGRAM_ID
        ),
        // Initialize the Mint
        createInitializeMintInstruction(
            mintKeypair.publicKey,
            decimals,
            payer.publicKey, // Mint Authority
            payer.publicKey, // Freeze Authority (for vesting locks)
            TOKEN_2022_PROGRAM_ID
        )
    );

    // 3. Send and confirm the transaction
    const signature = await sendAndConfirmTransaction(
        connection,
        transaction,
        [payer, mintKeypair],
        { commitment: 'confirmed' }
    );
    console.log(`Mint Initialization Transaction: ${signature}`);

    return { mint: mintKeypair.publicKey, totalSupply };
}

// Function to revoke mint authority to cap supply permanently at Genesis
export async function revokeMintAuthority(
    connection: Connection,
    payer: Keypair,
    mintPubkey: any
) {
    const transaction = new Transaction().add(
        createSetAuthorityInstruction(
            mintPubkey,
            payer.publicKey,
            AuthorityType.MintTokens,
            null, // Setting to null permanently revokes the authority
            [],
            TOKEN_2022_PROGRAM_ID
        )
    );

    const signature = await sendAndConfirmTransaction(
        connection,
        transaction,
        [payer],
        { commitment: 'confirmed' }
    );
    console.log(`Mint Authority Revoked: ${signature}`);
}
