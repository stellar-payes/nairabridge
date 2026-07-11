'use client';

import { Networks, StellarWalletsKit } from '@creit.tech/stellar-wallets-kit';
import { AlbedoModule } from '@creit.tech/stellar-wallets-kit/modules/albedo';
import { FreighterModule, FREIGHTER_ID } from '@creit.tech/stellar-wallets-kit/modules/freighter';
import { LobstrModule } from '@creit.tech/stellar-wallets-kit/modules/lobstr';
import { xBullModule } from '@creit.tech/stellar-wallets-kit/modules/xbull';

const NETWORK = process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE?.includes('Public')
  ? Networks.PUBLIC
  : Networks.TESTNET;

let initialized = false;

// The kit is a static singleton in v2; init exactly once, lazily, so this
// module can be imported during SSR without touching browser APIs.
function ensureKit() {
  if (initialized) return;
  StellarWalletsKit.init({
    network: NETWORK,
    selectedWalletId: FREIGHTER_ID,
    modules: [new FreighterModule(), new xBullModule(), new AlbedoModule(), new LobstrModule()],
  });
  initialized = true;
}

export async function connectWallet(): Promise<string> {
  ensureKit();
  // authModal lets the user pick a wallet, sets it active, and returns the
  // address in one step.
  const { address } = await StellarWalletsKit.authModal();
  return address;
}

export async function signTransaction(xdr: string, address: string): Promise<string> {
  ensureKit();
  const { signedTxXdr } = await StellarWalletsKit.signTransaction(xdr, {
    address,
    networkPassphrase: process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ?? Networks.TESTNET,
  });
  return signedTxXdr;
}
