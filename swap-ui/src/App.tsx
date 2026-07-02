import { useState, useMemo } from 'react';
import { ConnectionProvider, WalletProvider } from '@solana/wallet-adapter-react';
import { WalletModalProvider, WalletMultiButton } from '@solana/wallet-adapter-react-ui';
import { PhantomWalletAdapter, SolflareWalletAdapter } from '@solana/wallet-adapter-wallets';
import { clusterApiUrl } from '@solana/web3.js';
import { Header } from './components/Header';
import { SwapWidget } from './components/SwapWidget';
import { OtcPortal } from './components/OtcPortal';
import '@solana/wallet-adapter-react-ui/styles.css';

function App() {
  const [activeTab, setActiveTab] = useState<'swap' | 'otc'>('swap');

  // Set up Solana network connection
  const network = clusterApiUrl('devnet');
  const endpoint = import.meta.env.VITE_SOLANA_RPC_URL || network;
  const wallets = useMemo(
    () => [new PhantomWalletAdapter(), new SolflareWalletAdapter()],
    []
  );

  return (
    <ConnectionProvider endpoint={endpoint}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>
          <div className="app-container">
            <Header />
            <div style={{ display: 'flex', justifyContent: 'center', marginBottom: '16px' }}>
              <WalletMultiButton />
            </div>
      
      <div style={{ display: 'flex', justifyContent: 'center', gap: '16px', marginBottom: '32px' }}>
        <button 
          onClick={() => setActiveTab('swap')}
          style={{ 
            background: activeTab === 'swap' ? 'var(--primary)' : 'transparent',
            color: activeTab === 'swap' ? '#000' : 'var(--text-muted)',
            border: activeTab === 'swap' ? 'none' : '1px solid var(--glass-border)',
            padding: '10px 24px',
            borderRadius: '20px',
            fontWeight: '600',
            cursor: 'pointer',
            transition: 'all 0.3s'
          }}
        >
          Universal Swap
        </button>
        <button 
          onClick={() => setActiveTab('otc')}
          style={{ 
            background: activeTab === 'otc' ? 'var(--primary)' : 'transparent',
            color: activeTab === 'otc' ? '#000' : 'var(--text-muted)',
            border: activeTab === 'otc' ? 'none' : '1px solid var(--glass-border)',
            padding: '10px 24px',
            borderRadius: '20px',
            fontWeight: '600',
            cursor: 'pointer',
            transition: 'all 0.3s'
          }}
        >
          OTC Draw Pool
        </button>
      </div>

      <main>
        {activeTab === 'swap' ? <SwapWidget /> : <OtcPortal />}
      </main>
    </div>
        </WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}

export default App;

