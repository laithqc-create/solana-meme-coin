import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Header } from './Header';

// Mock the wallet adapter context to prevent errors during rendering
vi.mock('@solana/wallet-adapter-react', () => ({
  useWallet: () => ({
    wallet: null,
    connect: vi.fn(),
    disconnect: vi.fn(),
    select: vi.fn(),
  }),
}));

// Mock the wallet button UI
vi.mock('@solana/wallet-adapter-react-ui', () => ({
  WalletMultiButton: () => <button data-testid="wallet-multi-button">Select Wallet</button>,
}));

describe('Header Component', () => {
  it('renders the branding text correctly', () => {
    render(<Header />);
    expect(screen.getByText('MemeCoin DEX')).toBeInTheDocument();
  });

  it('renders the WalletMultiButton component', () => {
    render(<Header />);
    expect(screen.getByTestId('wallet-multi-button')).toBeInTheDocument();
  });
});
