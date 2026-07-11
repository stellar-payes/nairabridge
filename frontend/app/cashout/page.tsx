'use client';

import { useState } from 'react';
import { getWithdrawUrl } from '@/lib/api';
import { authenticateWithAnchor } from '@/lib/sep10';
import { useWallet } from '@/components/WalletProvider';

const ASSETS = ['NGNC', 'USDC', 'EURC'];
const HOME_DOMAIN = process.env.NEXT_PUBLIC_ANCHOR_HOME_DOMAIN ?? 'testanchor.stellar.org';

export default function CashoutPage() {
  const { address, connect } = useWallet();
  const [assetCode, setAssetCode] = useState('NGNC');
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function handleCashout() {
    if (!address) return connect();

    setBusy(true);
    setStatus('Authenticating with anchor…');
    try {
      const sep10Token = await authenticateWithAnchor(address, HOME_DOMAIN);
      setStatus('Opening bank transfer form…');
      const { url } = await getWithdrawUrl({
        account: address,
        assetCode,
        sep10Token,
        homeDomain: HOME_DOMAIN,
      });
      window.open(url, '_blank', 'noopener,noreferrer');
      setStatus('Complete your bank details in the new tab, then funds will be withdrawn automatically.');
    } catch (err) {
      setStatus(`Failed: ${(err as Error).message}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto max-w-md space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Cash Out</h1>
        <p className="text-sm text-neutral-500">
          Withdraw straight to your bank account via our Stellar anchor partner.
        </p>
      </div>

      <div className="space-y-1">
        <label className="text-sm font-medium text-neutral-700">Asset to withdraw</label>
        <select
          value={assetCode}
          onChange={(e) => setAssetCode(e.target.value)}
          className="w-full rounded-xl border border-neutral-300 px-4 py-3"
        >
          {ASSETS.map((a) => (
            <option key={a}>{a}</option>
          ))}
        </select>
      </div>

      <button
        onClick={handleCashout}
        disabled={busy}
        className="w-full rounded-xl bg-brand-600 py-3 text-lg font-semibold text-white hover:bg-brand-700 disabled:opacity-50"
      >
        {!address ? 'Connect Wallet' : busy ? 'Working…' : 'Start Bank Withdrawal'}
      </button>

      {status && <p className="text-center text-sm text-neutral-600">{status}</p>}
    </div>
  );
}
