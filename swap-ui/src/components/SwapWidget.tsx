import { useState, useEffect } from 'react';

// Common Solana token mints
const SOL_MINT = 'So11111111111111111111111111111111111111112';
// We will mock our meme coin mint here, usually loaded from .env
const MEME_MINT = import.meta.env.VITE_MEME_COIN_MINT_ADDRESS || 'TokenVesting1111111111111111111111111111111';

export function SwapWidget() {
  const [payAmount, setPayAmount] = useState<string>('');
  const [receiveAmount, setReceiveAmount] = useState<string>('');
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  // Debounce the API call
  useEffect(() => {
    const fetchQuote = async () => {
      if (!payAmount || isNaN(Number(payAmount)) || Number(payAmount) <= 0) {
        setReceiveAmount('');
        setError(null);
        return;
      }

      setIsLoading(true);
      setError(null);

      try {
        // Amount is in lamports (9 decimals for SOL)
        const amountLamports = Math.floor(Number(payAmount) * 1e9);
        
        // Call Jupiter v6 Quote API
        const response = await fetch(
          `https://quote-api.jup.ag/v6/quote?inputMint=${SOL_MINT}&outputMint=${MEME_MINT}&amount=${amountLamports}&slippageBps=50`
        );

        if (!response.ok) {
          throw new Error('Failed to fetch quote');
        }

        const data = await response.json();
        
        if (data && data.outAmount) {
          // Assuming output is also 9 decimals
          const outAmountFormatted = (Number(data.outAmount) / 1e9).toFixed(4);
          setReceiveAmount(outAmountFormatted);
        } else {
          setReceiveAmount('0.00');
        }
      } catch (err: any) {
        console.error(err);
        setError('Route not found or liquidity too low.');
        setReceiveAmount('');
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
    // Note: A true swap would require updating the input/output mint states as well.
    // For now, this just switches the UI fields visually.
    setPayAmount(receiveAmount);
    setReceiveAmount('');
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
        
        {error && <div style={{ color: '#ff4444', marginTop: '12px', fontSize: '0.9rem', textAlign: 'center' }}>{error}</div>}

        <button className="primary-button" style={{ marginTop: '24px' }}>
          Swap Tokens
        </button>
      </div>
    </div>
  );
}
