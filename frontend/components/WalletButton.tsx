'use client';

import { useWallet } from './WalletProvider';

function truncate(address: string) {
  return `${address.slice(0, 4)}…${address.slice(-4)}`;
}

export function WalletButton() {
  const { address, connecting, connect, disconnect } = useWallet();

  if (address) {
    return (
      <button
        onClick={disconnect}
        className="rounded-full border border-brand-600 px-4 py-2 text-sm font-medium text-brand-700 hover:bg-brand-50"
      >
        {truncate(address)}
      </button>
    );
  }

  return (
    <button
      onClick={() => connect()}
      disabled={connecting}
      className="rounded-full bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-60"
    >
      {connecting ? 'Connecting…' : 'Connect Wallet'}
    </button>
  );
}
