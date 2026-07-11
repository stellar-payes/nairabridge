'use client';

import {
  Account,
  Address,
  Contract,
  Keypair,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
  type xdr,
} from '@stellar/stellar-sdk';

const RPC_URL = process.env.NEXT_PUBLIC_SOROBAN_RPC_URL ?? 'https://soroban-testnet.stellar.org';
const NETWORK_PASSPHRASE =
  process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ?? 'Test SDF Network ; September 2015';

export const server = new rpc.Server(RPC_URL);

export function scAddress(value: string) {
  return new Address(value).toScVal();
}

export function scI128(value: bigint) {
  return nativeToScVal(value, { type: 'i128' });
}

export function scAddressVec(values: string[]) {
  return nativeToScVal(values.map((v) => new Address(v)));
}

export function scI128Vec(values: bigint[]) {
  return nativeToScVal(values.map((v) => nativeToScVal(v, { type: 'i128' })));
}

export function scU32(value: number) {
  return nativeToScVal(value, { type: 'u32' });
}

/**
 * Builds a transaction from one or more contract operations, has the
 * connected wallet sign it, submits it, and polls until it lands. Used by
 * every "Confirm" button across /send, /swap, and /pool.
 */
export async function buildSignAndSubmit(
  publicKey: string,
  ops: ReturnType<Contract['call']>[],
  signFn: (xdr: string, address: string) => Promise<string>
) {
  const account = await server.getAccount(publicKey);
  let builder = new TransactionBuilder(account, {
    fee: '1000000', // ceiling only -- prepareTransaction resolves the real Soroban resource fee
    networkPassphrase: NETWORK_PASSPHRASE,
  });
  for (const op of ops) {
    builder = builder.addOperation(op);
  }
  const built = builder.setTimeout(60).build();

  const prepared = await server.prepareTransaction(built);
  const signedXdr = await signFn(prepared.toXDR(), publicKey);
  const signedTx = TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE);

  const sendResult = await server.sendTransaction(signedTx);
  if (sendResult.status === 'ERROR') {
    throw new Error(`Transaction submission failed: ${JSON.stringify(sendResult.errorResult)}`);
  }
  return pollTransaction(sendResult.hash);
}

/**
 * Simulated (unsigned, unsubmitted) read-only contract call -- used for
 * quick UI reads like "what's my LP balance" without a wallet prompt. Any
 * validly-formatted account works as the simulation source since nothing
 * is actually executed on-chain.
 */
export async function readContract(contractId: string, method: string, args: xdr.ScVal[] = []) {
  const source = new Account(Keypair.random().publicKey(), '0');
  const contract = new Contract(contractId);
  const tx = new TransactionBuilder(source, { fee: '100', networkPassphrase: NETWORK_PASSPHRASE })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`Simulation failed for ${contractId}.${method}: ${sim.error}`);
  }
  return scValToNative(sim.result!.retval);
}

async function pollTransaction(hash: string) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const res = await server.getTransaction(hash);
    if (res.status !== rpc.Api.GetTransactionStatus.NOT_FOUND) {
      return res;
    }
    await new Promise((resolve) => setTimeout(resolve, 1500));
  }
  throw new Error(`Timed out waiting for transaction ${hash} to confirm`);
}
