'use client';

import { useEffect, useState } from 'react';
import { Contract } from '@stellar/stellar-sdk';
import { getQuote, type QuoteResponse } from '@/lib/api';
import { buildSignAndSubmit, scAddress, scAddressVec, scI128 } from '@/lib/soroban';
import { signTransaction } from '@/lib/walletKit';
import { useWallet } from '@/components/WalletProvider';

const DECIMALS = 10_000_000n;
const ASSETS = ['USDC', 'NGNC', 'EURC'];
const ROUTER_ID = process.env.NEXT_PUBLIC_ROUTER_CONTRACT_ID ?? '';

export default function SwapPage() {
  const { address, connect } = useWallet();
  const [tokenIn, setTokenIn] = useState('USDC');
  const [tokenOut, setTokenOut] = useState('NGNC');
  const [amountIn, setAmountIn] = useState('');
  const [slippagePct, setSlippagePct] = useState(0.5);
  const [quote, setQuote] = useState<QuoteResponse | null>(null);
  const [quoteError, setQuoteError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    const amount = Number(amountIn);
    if (!amount || amount <= 0 || tokenIn === tokenOut) {
      setQuote(null);
      return;
    }
    const handle = setTimeout(() => {
      getQuote(tokenIn, tokenOut, amount)
        .then((q) => {
          setQuote(q);
          setQuoteError(null);
        })
        .catch((err) => {
          setQuote(null);
          setQuoteError(err.message);
        });
    }, 350);
    return () => clearTimeout(handle);
  }, [amountIn, tokenIn, tokenOut]);

  function flip() {
    setTokenIn(tokenOut);
    setTokenOut(tokenIn);
    setQuote(null);
  }

  async function handleConfirm() {
    if (!address) {
      await connect();
      return;
    }
    if (!quote || !ROUTER_ID) return;

    setSubmitting(true);
    setStatus(null);
    try {
      const amountInRaw = BigInt(Math.round(Number(amountIn) * Number(DECIMALS)));
      const minOut = BigInt(
        Math.floor(quote.amountOut * Number(DECIMALS) * (1 - slippagePct / 100))
      );

      const router = new Contract(ROUTER_ID);
      setStatus('Submitting transaction — confirm in your wallet…');
      await buildSignAndSubmit(
        address,
        [
          router.call(
            'swap_exact_in',
            scAddress(address),
            scAddressVec(quote.route),
            scAddressVec(quote.pools),
            scI128(amountInRaw),
            scI128(minOut)
          ),
        ],
        signTransaction
      );
      setStatus('Swap complete!');
      setAmountIn('');
      setQuote(null);
    } catch (err) {
      setStatus(`Failed: ${(err as Error).message}`);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="mx-auto max-w-md space-y-6">
      <h1 className="text-2xl font-semibold">Swap</h1>

      <div className="space-y-3 rounded-xl border border-neutral-200 p-4">
        <div className="flex items-center gap-2">
          <input
            type="number"
            min="0"
            step="0.01"
            value={amountIn}
            onChange={(e) => setAmountIn(e.target.value)}
            placeholder="0.00"
            className="w-full rounded-lg border border-neutral-300 px-3 py-2 text-lg focus:border-brand-500 focus:outline-none"
          />
          <select
            value={tokenIn}
            onChange={(e) => setTokenIn(e.target.value)}
            className="rounded-lg border border-neutral-300 px-3 py-2"
          >
            {ASSETS.map((a) => (
              <option key={a}>{a}</option>
            ))}
          </select>
        </div>

        <div className="flex justify-center">
          <button
            onClick={flip}
            className="rounded-full border border-neutral-300 px-3 py-1 text-sm text-neutral-500 hover:bg-neutral-100"
            aria-label="Flip tokens"
          >
            ↓↑
          </button>
        </div>

        <div className="flex items-center gap-2">
          <input
            type="text"
            readOnly
            value={quote ? quote.amountOut.toFixed(4) : ''}
            placeholder="0.00"
            className="w-full rounded-lg border border-neutral-200 bg-neutral-50 px-3 py-2 text-lg"
          />
          <select
            value={tokenOut}
            onChange={(e) => setTokenOut(e.target.value)}
            className="rounded-lg border border-neutral-300 px-3 py-2"
          >
            {ASSETS.map((a) => (
              <option key={a}>{a}</option>
            ))}
          </select>
        </div>
      </div>

      <div className="flex items-center justify-between text-sm text-neutral-600">
        <span>Slippage tolerance</span>
        <div className="flex gap-1">
          {[0.1, 0.5, 1].map((v) => (
            <button
              key={v}
              onClick={() => setSlippagePct(v)}
              className={`rounded-full px-3 py-1 ${
                slippagePct === v ? 'bg-brand-600 text-white' : 'bg-neutral-100 hover:bg-neutral-200'
              }`}
            >
              {v}%
            </button>
          ))}
        </div>
      </div>

      {quoteError && <p className="text-sm text-red-600">{quoteError}</p>}
      {quote && (
        <p className="text-center text-xs text-neutral-500">
          Price impact: {(quote.priceImpact * 100).toFixed(3)}% · Route: {quote.route.length - 1} hop
          {quote.route.length - 1 > 1 ? 's' : ''}
        </p>
      )}

      <button
        onClick={handleConfirm}
        disabled={submitting || (!!address && !quote)}
        className="w-full rounded-xl bg-brand-600 py-3 text-lg font-semibold text-white hover:bg-brand-700 disabled:opacity-50"
      >
        {!address ? 'Connect Wallet' : submitting ? 'Swapping…' : 'Confirm Swap'}
      </button>

      {status && <p className="text-center text-sm text-neutral-600">{status}</p>}
    </div>
  );
}
