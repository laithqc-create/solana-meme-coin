import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';

export function Header() {
  return (
    <header className="header">
      <div className="logo">MemeCoin DEX</div>
      <WalletMultiButton className="wallet-btn" />
    </header>
  );
}
