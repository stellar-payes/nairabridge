'use client';

import { useEffect, useState } from 'react';
import { Contract } from '@stellar/stellar-sdk';
import { getPools, getPoolStats, type PoolStats, type PoolSummary } from '@/lib/api';
import {
  buildSignAndSubmit,
  readContract,
  scAddress,
  scI128,
  scI128Vec,
  scU32,
  server,
} from '@/lib/soroban';
import { signTransaction } from '@/lib/walletKit';
import { useWallet } from '@/components/WalletProvider';

const DECIMALS = 10_000_000n;
const DEPOSIT_SLIPPAGE = 0.005; // 0.5%
const LP_APPROVE_LEDGER_WINDOW = 100;

export default function PoolPage() {
  const { address, connect } = useWallet();
  const [pools, setPools] = useState<PoolSummary[]>([]);
  const [selected, setSelected] = useState<PoolStats | null>(null);
  const [lpBalance, setLpBalance] = useState<number | null>(null);
  const [depositAmounts, setDepositAmounts] = useState<string[]>([]);
  const [withdrawAmount, setWithdrawAmount] = useState('');
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    getPools().then(setPools).catch(() => setPools([]));
  }, []);

  async function selectPool(id: string) {
    const stats = await getPoolStats(id);
    setSelected(stats);
    setDepositAmounts(stats.tokens.map(() => ''));
    setLpBalance(null);
    if (address) {
      const balance = await readContract(stats.lpToken, 'balance', [scAddress(address)]);
      setLpBalance(Number(balance) / Number(DECIMALS));
    }
  }

  async function handleDeposit() {
    if (!address) return connect();
    if (!selected) return;

    setBusy(true);
    setStatus(null);
    try {
      const amounts = depositAmounts.map((a) => BigInt(Math.round(Number(a || '0') * Number(DECIMALS))));
      const sumIn = amounts.reduce((a, b) => a + b, 0n);
      // Rough floor for the minimum LP minted -- a true expectation needs
      // the stable-swap invariant computed client-side, which we skip here.
      // Fine for a balanced deposit; pad more for skewed ones.
      const minLp = (sumIn * BigInt(Math.floor((1 - DEPOSIT_SLIPPAGE) * 1000))) / 1000n;

      const pool = new Contract(selected.id);
      await buildSignAndSubmit(
        address,
        [pool.call('add_liquidity', scAddress(address), scI128Vec(amounts), scI128(minLp))],
        signTransaction
      );
      setStatus('Liquidity added!');
      await selectPool(selected.id);
    } catch (err) {
      setStatus(`Failed: ${(err as Error).message}`);
    } finally {
      setBusy(false);
    }
  }

  async function handleWithdraw() {
    if (!address) return connect();
    if (!selected) return;

    setBusy(true);
    setStatus(null);
    try {
      const lpAmount = BigInt(Math.round(Number(withdrawAmount || '0') * Number(DECIMALS)));
      const minAmounts = selected.tokens.map(() => 0n);
      const ledger = await server.getLatestLedger();

      const lpToken = new Contract(selected.lpToken);
      const pool = new Contract(selected.id);

      await buildSignAndSubmit(
        address,
        [
          // LP tokens live in the user's own wallet, so the pool needs a
          // prior approval before it can burn them on withdrawal -- batched
          // into the same transaction as remove_liquidity.
          lpToken.call(
            'approve',
            scAddress(address),
            scAddress(selected.id),
            scI128(lpAmount),
            scU32(ledger.sequence + LP_APPROVE_LEDGER_WINDOW)
          ),
          pool.call('remove_liquidity', scAddress(address), scI128(lpAmount), scI128Vec(minAmounts)),
        ],
        signTransaction
      );
      setStatus('Withdrawal complete!');
      setWithdrawAmount('');
      await selectPool(selected.id);
    } catch (err) {
      setStatus(`Failed: ${(err as Error).message}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <h1 className="text-2xl font-semibold">Liquidity Pools</h1>

      <div className="grid gap-3 sm:grid-cols-2">
        {pools.map((p) => (
          <button
            key={p.id}
            onClick={() => selectPool(p.id)}
            className={`rounded-xl border p-4 text-left hover:border-brand-500 ${
              selected?.id === p.id ? 'border-brand-600 bg-brand-50' : 'border-neutral-200'
            }`}
          >
            <p className="font-medium">{p.tokenSymbols.join(' / ')}</p>
            <p className="text-xs text-neutral-500">
              Fee {(p.feeBps / 100).toFixed(2)}% · Amp {p.amp}
            </p>
          </button>
        ))}
        {pools.length === 0 && <p className="text-sm text-neutral-500">No pools registered yet.</p>}
      </div>

      {selected && (
        <div className="space-y-6 rounded-xl border border-neutral-200 p-5">
          <div className="grid grid-cols-3 gap-4 text-center">
            <Stat
              label="TVL"
              value={`$${selected.tvl.toLocaleString(undefined, { maximumFractionDigits: 0 })}`}
            />
            <Stat
              label="24h Volume"
              value={`$${selected.volume24h.toLocaleString(undefined, { maximumFractionDigits: 0 })}`}
            />
            <Stat label="APY" value={`${selected.apy.toFixed(2)}%`} />
          </div>

          <div className="space-y-3">
            <h2 className="font-medium">Deposit</h2>
            {selected.tokenSymbols.map((symbol, i) => (
              <div key={symbol} className="flex items-center gap-2">
                <input
                  type="number"
                  min="0"
                  step="0.01"
                  value={depositAmounts[i] ?? ''}
                  onChange={(e) => {
                    const next = [...depositAmounts];
                    next[i] = e.target.value;
                    setDepositAmounts(next);
                  }}
                  placeholder="0.00"
                  className="w-full rounded-lg border border-neutral-300 px-3 py-2"
                />
                <span className="w-16 text-sm text-neutral-500">{symbol}</span>
              </div>
            ))}
            <button
              onClick={handleDeposit}
              disabled={busy}
              className="w-full rounded-lg bg-brand-600 py-2 font-medium text-white hover:bg-brand-700 disabled:opacity-50"
            >
              {!address ? 'Connect Wallet' : 'Add Liquidity'}
            </button>
          </div>

          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="font-medium">Withdraw</h2>
              {lpBalance !== null && (
                <span className="text-xs text-neutral-500">Balance: {lpBalance.toFixed(4)} LP</span>
              )}
            </div>
            <div className="flex items-center gap-2">
              <input
                type="number"
                min="0"
                step="0.01"
                value={withdrawAmount}
                onChange={(e) => setWithdrawAmount(e.target.value)}
                placeholder="0.00"
                className="w-full rounded-lg border border-neutral-300 px-3 py-2"
              />
              {lpBalance !== null && (
                <button
                  onClick={() => setWithdrawAmount(String(lpBalance))}
                  className="text-xs text-brand-600 hover:underline"
                >
                  Max
                </button>
              )}
            </div>
            <button
              onClick={handleWithdraw}
              disabled={busy}
              className="w-full rounded-lg border border-brand-600 py-2 font-medium text-brand-700 hover:bg-brand-50 disabled:opacity-50"
            >
              {!address ? 'Connect Wallet' : 'Remove Liquidity'}
            </button>
          </div>

          {status && <p className="text-center text-sm text-neutral-600">{status}</p>}
        </div>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-lg font-semibold">{value}</p>
      <p className="text-xs text-neutral-500">{label}</p>
    </div>
  );
}
