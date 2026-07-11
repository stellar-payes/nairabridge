'use client';

import { useEffect, useState } from 'react';
import { StrKey, Contract } from '@stellar/stellar-sdk';
import { getQuote, type QuoteResponse } from '@/lib/api';
import { buildSignAndSubmit, scAddress, scAddressVec, scI128 } from '@/lib/soroban';
import { signTransaction } from '@/lib/walletKit';
import { useWallet } from '@/components/WalletProvider';

const SEND_ASSET = 'USDC';
const RECEIVE_ASSET = 'NGNC';
const DECIMALS = 10_000_000n;
const SLIPPAGE_TOLERANCE = 0.005; // 0.5%
const ROUTER_ID = process.env.NEXT_PUBLIC_ROUTER_CONTRACT_ID ?? '';

async function resolveRecipient(input: string): Promise<string> {
  if (StrKey.isValidEd25519PublicKey(input)) return input;

  if (input.includes('*')) {
    const [name, domain] = input.split('*');
    const tomlRes = await fetch(`https://${domain}/.well-known/stellar.toml`);
    const toml = await tomlRes.text();
    const match = toml.match(/FEDERATION_SERVER\s*=\s*"([^"]+)"/);
    if (!match) throw new Error(`${domain} does not advertise a federation server`);
    const fedRes = await fetch(`${match[1]}?q=${encodeURIComponent(input)}&type=name`);
    if (!fedRes.ok) throw new Error(`federation lookup failed for ${name}*${domain}`);
    const body = await fedRes.json();
    if (!StrKey.isValidEd25519PublicKey(body.account_id)) {
      throw new Error('federation server returned an invalid account id');
    }
    return body.account_id;
  }

  throw new Error('enter a Stellar address (G...) or federation address (name*domain)');
}

export default function SendPage() {
  const { address, connect } = useWallet();
  const [amountUsd, setAmountUsd] = useState('');
  const [recipient, setRecipient] = useState('');
  const [quote, setQuote] = useState<QuoteResponse | null>(null);
  const [quoteError, setQuoteError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    const amount = Number(amountUsd);
    if (!amount || amount <= 0) {
      setQuote(null);
      return;
    }
    const handle = setTimeout(() => {
      getQuote(SEND_ASSET, RECEIVE_ASSET, amount)
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
  }, [amountUsd]);

  async function handleConfirm() {
    if (!address) {
      await connect();
      return;
    }
    if (!quote || !ROUTER_ID) return;

    setSubmitting(true);
    setStatus(null);
    try {
      const recipientAddress = await resolveRecipient(recipient);

      const amountIn = BigInt(Math.round(Number(amountUsd) * Number(DECIMALS)));
      const minOut = BigInt(
        Math.floor(quote.amountOut * Number(DECIMALS) * (1 - SLIPPAGE_TOLERANCE))
      );
      const ngncToken = quote.route[quote.route.length - 1];

      const router = new Contract(ROUTER_ID);
      const ngnc = new Contract(ngncToken);

      setStatus('Submitting transaction — confirm in your wallet…');
      await buildSignAndSubmit(
        address,
        [
          router.call(
            'swap_exact_in',
            scAddress(address),
            scAddressVec(quote.route),
            scAddressVec(quote.pools),
            scI128(amountIn),
            scI128(minOut)
          ),
          // Swap output lands back in the sender's own wallet, so a second
          // op in the same atomic transaction forwards exactly `minOut` on
          // to the recipient -- guaranteed to succeed since the swap op
          // enforces at least that much output.
          ngnc.call('transfer', scAddress(address), scAddress(recipientAddress), scI128(minOut)),
        ],
        signTransaction
      );
      setStatus('Sent! Funds are on their way.');
      setAmountUsd('');
      setRecipient('');
      setQuote(null);
    } catch (err) {
      setStatus(`Failed: ${(err as Error).message}`);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="mx-auto max-w-md space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Send Money</h1>
        <p className="text-sm text-neutral-500">
          Enter an amount and where it's going — we handle the swap.
        </p>
      </div>

      <div className="space-y-1">
        <label className="text-sm font-medium text-neutral-700">You send (USD)</label>
        <input
          type="number"
          min="0"
          step="0.01"
          value={amountUsd}
          onChange={(e) => setAmountUsd(e.target.value)}
          placeholder="100.00"
          className="w-full rounded-xl border border-neutral-300 px-4 py-3 text-lg focus:border-brand-500 focus:outline-none"
        />
      </div>

      <div className="space-y-1">
        <label className="text-sm font-medium text-neutral-700">Recipient</label>
        <input
          type="text"
          value={recipient}
          onChange={(e) => setRecipient(e.target.value)}
          placeholder="G... or name*domain"
          className="w-full rounded-xl border border-neutral-300 px-4 py-3 text-sm focus:border-brand-500 focus:outline-none"
        />
      </div>

      <div className="rounded-xl bg-brand-50 p-4 text-center">
        {quoteError && <p className="text-sm text-red-600">{quoteError}</p>}
        {!quoteError && quote && (
          <p className="text-lg font-semibold text-brand-700">
            Recipient gets ₦{quote.amountOut.toLocaleString(undefined, { maximumFractionDigits: 2 })}
          </p>
        )}
        {!quoteError && !quote && <p className="text-sm text-neutral-500">Enter an amount to see a quote</p>}
      </div>

      <button
        onClick={handleConfirm}
        disabled={submitting || (!!address && (!quote || !recipient))}
        className="w-full rounded-xl bg-brand-600 py-3 text-lg font-semibold text-white hover:bg-brand-700 disabled:opacity-50"
      >
        {!address ? 'Connect Wallet' : submitting ? 'Sending…' : 'Confirm & Send'}
      </button>

      {status && <p className="text-center text-sm text-neutral-600">{status}</p>}
    </div>
  );
}
