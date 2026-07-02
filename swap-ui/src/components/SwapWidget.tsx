import { useState, useEffect, useCallback } from 'react';
import { useConnection, useWallet } from '@solana/wallet-adapter-react';
import { buildTaxedSwapTransaction, MEME_MINT } from '../lib/swapTransaction';

// Common Solana token mints
const SOL_MINT = 'So11111111111111111111111111111111111111112';
const MEME_MINT_STR = MEME_MINT.toBase58();

type SwapStatus = 'idle' | 'building' | 'awaiting-signature' | 'sending' | 'confirming' | 'success' | 'error';

export function SwapWidget() {
  const { connection } = useConnection();
  const { publicKey, signTransaction, connected } = useWallet();

  const [payAmount, setPayAmount] = useState<string>('');
  const [receiveAmount, setReceiveAmount] = useState<string>('');
  const [quote, setQuote] = useState<any | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<SwapStatus>('idle');
  const [txSignature, setTxSignature] = useState<string | null>(null);

  // Debounce the quote API call
  useEffect(() => {
    const fetchQuote = async () => {
      if (!payAmount || isNaN(Number(payAmount)) || Number(payAmount) <= 0) {
        setReceiveAmount('');
        setQuote(null);
        setError(null);
        return;
      }

      setIsLoading(true);
      setError(null);

      try {
        const amountLamports = Math.floor(Number(payAmount) * 1e9);

        const response = await fetch(
          `https://quote-api.jup.ag/v6/quote?inputMint=${SOL_MINT}&outputMint=${MEME_MINT_STR}&amount=${amountLamports}&slippageBps=50`
        );

        if (!response.ok) {
          throw new Error('Failed to fetch quote');
        }

        const data = await response.json();
        setQuote(data);

        if (data && data.outAmount) {
          const outAmountFormatted = (Number(data.outAmount) / 1e9).toFixed(4);
          setReceiveAmount(outAmountFormatted);
        } else {
          setReceiveAmount('0.00');
        }
      } catch (err: any) {
        console.error(err);
        setError('Route not found or liquidity too low.');
        setReceiveAmount('');
        setQuote(null);
      } finally {
        setIsLoading(false);
      }
    };

    const delayDebounceFn = setTimeout(() => {
      fetchQuote();
    }, 500);

    return () => clearTimeout(delayDebounceFn);
  }, [payAmount]);

  const handleSwapValues = () => {
    // Note: A true swap-direction toggle would also need to swap input/output
    // mints and re-fetch a quote in that direction. Left as a follow-up —
    // this widget currently only supports SOL -> MEME.
    setPayAmount(receiveAmount);
    setReceiveAmount('');
  };

  const handleSwap = useCallback(async () => {
    if (!connected || !publicKey || !signTransaction) {
      setError('Connect a wallet first.');
      return;
    }
    if (!quote) {
      setError('No quote available yet.');
      return;
    }

    setError(null);
    setTxSignature(null);

    try {
      setStatus('building');
      const amountLamports = BigInt(Math.floor(Number(payAmount) * 1e9));

      const tx = await buildTaxedSwapTransaction({
        connection,
        userPublicKey: publicKey,
        quoteResponse: quote,
        inputMint: SOL_MINT,
        outputMint: MEME_MINT_STR,
        amountLamports,
      });

      setStatus('awaiting-signature');
      const signedTx = await signTransaction(tx);

      setStatus('sending');
      const signature = await connection.sendRawTransaction(signedTx.serialize(), {
        skipPreflight: false,
        maxRetries: 3,
      });
      setTxSignature(signature);

      setStatus('confirming');
      const latestBlockhash = await connection.getLatestBlockhash();
      await connection.confirmTransaction(
        { signature, ...latestBlockhash },
        'confirmed'
      );

      setStatus('success');
    } catch (err: any) {
      console.error(err);
      setError(err?.message || 'Swap failed.');
      setStatus('error');
    }
  }, [connected, publicKey, signTransaction, quote, payAmount, connection]);

  const statusLabel: Record<SwapStatus, string> = {
    idle: 'Swap Tokens',
    building: 'Building transaction...',
    'awaiting-signature': 'Confirm in wallet...',
    sending: 'Sending...',
    confirming: 'Confirming...',
    success: 'Swap complete ✓',
    error: 'Swap Tokens',
  };

  return (
    <div className="swap-widget-container">
      <div className="glass-panel swap-widget">
        <div className="swap-header">
          <span className="swap-title">Swap</span>
        </div>

        <div className="swap-input-group">
          <label className="token-label">You Pay</label>
          <input
            type="number"
            className="input-field"
            placeholder="0.00"
            value={payAmount}
            onChange={(e) => setPayAmount(e.target.value)}
          />
          <button className="token-selector">
            ◎ SOL
          </button>
        </div>

        <div className="swap-divider">
          <button className="icon-button" onClick={handleSwapValues}>
            ↓
          </button>
        </div>

        <div className="swap-input-group">
          <label className="token-label">You Receive</label>
          <input
            type="number"
            className="input-field"
            placeholder={isLoading ? "Fetching best route..." : "0.00"}
            value={receiveAmount}
            readOnly
          />
          <button className="token-selector">
            MEME
          </button>
        </div>

        {quote && (
          <div style={{ marginTop: '8px', fontSize: '0.8rem', color: 'var(--text-muted)', textAlign: 'right' }}>
            includes 1% on-chain trade tax
          </div>
        )}

        {error && <div style={{ color: '#ff4444', marginTop: '12px', fontSize: '0.9rem', textAlign: 'center' }}>{error}</div>}

        {txSignature && status === 'success' && (
          <div style={{ marginTop: '12px', fontSize: '0.85rem', textAlign: 'center', wordBreak: 'break-all' }}>
            <a
              href={`https://solscan.io/tx/${txSignature}?cluster=devnet`}
              target="_blank"
              rel="noreferrer"
            >
              View transaction
            </a>
          </div>
        )}

        {!connected ? (
          <div style={{ marginTop: '24px', textAlign: 'center', color: 'var(--text-muted)', fontSize: '0.9rem' }}>
            Connect your wallet above to swap.
          </div>
        ) : (
          <button
            className="primary-button"
            style={{ marginTop: '24px' }}
            disabled={!quote || status === 'building' || status === 'awaiting-signature' || status === 'sending' || status === 'confirming'}
            onClick={handleSwap}
          >
            {statusLabel[status]}
          </button>
        )}
      </div>
    </div>
  );
}
