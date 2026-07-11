'use client';

import { signTransaction } from './walletKit';

/**
 * Completes a SEP-10 "web authentication" challenge with a Stellar anchor:
 * fetch the challenge transaction, have the connected wallet sign it (never
 * submitted on-chain -- it's just a proof-of-key-ownership artifact), and
 * exchange the signature for a JWT the anchor accepts on its SEP-24
 * endpoints. Required before /anchor/deposit-url or /anchor/withdraw-url
 * will do anything useful.
 */
export async function authenticateWithAnchor(address: string, homeDomain: string): Promise<string> {
  const tomlRes = await fetch(`https://${homeDomain}/.well-known/stellar.toml`);
  if (!tomlRes.ok) throw new Error(`failed to fetch stellar.toml from ${homeDomain}`);
  const toml = await tomlRes.text();
  const match = toml.match(/WEB_AUTH_ENDPOINT\s*=\s*"([^"]+)"/);
  if (!match) throw new Error(`${homeDomain} does not advertise a WEB_AUTH_ENDPOINT`);
  const authEndpoint = match[1];

  const challengeRes = await fetch(`${authEndpoint}?account=${address}&home_domain=${homeDomain}`);
  if (!challengeRes.ok) throw new Error('failed to fetch SEP-10 challenge');
  const { transaction } = await challengeRes.json();

  const signedXdr = await signTransaction(transaction, address);

  const tokenRes = await fetch(authEndpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ transaction: signedXdr }),
  });
  if (!tokenRes.ok) throw new Error('anchor rejected SEP-10 signature');
  const { token } = await tokenRes.json();
  return token;
}
