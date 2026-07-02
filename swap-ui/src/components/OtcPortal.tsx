import { useState } from 'react';

export function OtcPortal() {
  const [allocationRequest, setAllocationRequest] = useState<string>('');
  const [isEntered, setIsEntered] = useState<boolean>(false);

  const handleDrawEntry = () => {
    if (!allocationRequest || Number(allocationRequest) <= 0) return;
    // Mock logic for entering the draw
    setIsEntered(true);
  };

  return (
    <div className="swap-widget-container" style={{ maxWidth: '600px' }}>
      <div className="glass-panel swap-widget">
        <div className="swap-header" style={{ justifyContent: 'center', flexDirection: 'column', gap: '8px' }}>
          <span className="swap-title" style={{ fontSize: '1.5rem', color: 'var(--primary)' }}>OTC Drawing Pool</span>
          <span style={{ fontSize: '0.9rem', color: 'var(--text-muted)' }}>Fair Distribution Draw for 0% Slippage Sales</span>
        </div>

        {!isEntered ? (
          <>
            <div className="swap-input-group" style={{ marginTop: '24px' }}>
              <label className="token-label">Requested Allocation (SOL)</label>
              <input 
                type="number" 
                className="input-field" 
                placeholder="Enter SOL amount" 
                value={allocationRequest}
                onChange={(e) => setAllocationRequest(e.target.value)}
              />
            </div>

            <div style={{ background: 'rgba(0, 0, 0, 0.3)', padding: '16px', borderRadius: '12px', margin: '24px 0', fontSize: '0.9rem', color: '#ccc' }}>
              <strong>Pool Statistics:</strong>
              <ul style={{ listStyleType: 'none', padding: '8px 0 0 0', margin: 0, display: 'flex', flexDirection: 'column', gap: '8px' }}>
                <li style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <span>Total Pool Size:</span>
                  <span style={{ color: 'white' }}>50,000,000 MEME</span>
                </li>
                <li style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <span>Fixed Rate:</span>
                  <span style={{ color: 'white' }}>1 SOL = 100,000 MEME</span>
                </li>
                <li style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <span>Total Entries:</span>
                  <span style={{ color: 'white' }}>1,245 Wallets</span>
                </li>
              </ul>
            </div>

            <button className="primary-button" onClick={handleDrawEntry}>
              Enter Draw
            </button>
          </>
        ) : (
          <div style={{ textAlign: 'center', padding: '40px 20px' }}>
            <div style={{ fontSize: '3rem', marginBottom: '16px' }}>🎟️</div>
            <h3 style={{ marginBottom: '12px', color: 'var(--primary)' }}>Entry Confirmed!</h3>
            <p style={{ color: 'var(--text-muted)', lineHeight: '1.5' }}>
              Your wallet has successfully entered the drawing pool for an allocation of <strong>{allocationRequest} SOL</strong>. 
              <br /><br />
              Winners will be drawn transparently on-chain. Check back later to see if your allocation was approved.
            </p>
            <button 
              className="wallet-btn" 
              style={{ marginTop: '32px' }}
              onClick={() => setIsEntered(false)}
            >
              Submit Another Entry
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
