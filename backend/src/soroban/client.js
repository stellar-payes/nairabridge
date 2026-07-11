import { Account, Contract, Keypair, TransactionBuilder, rpc, scValToNative } from '@stellar/stellar-sdk';
import { config } from '../config.js';

const server = new rpc.Server(config.sorobanRpcUrl, {
  allowHttp: config.sorobanRpcUrl.startsWith('http://'),
});

// Read-only simulated calls don't need a funded or even real account -- a
// fresh throwaway keypair gives us a validly-formatted source address to
// build the transaction envelope around.
const SIMULATION_SOURCE = Keypair.random().publicKey();

/**
 * Invokes a contract method via `simulateTransaction` (no submission, no
 * fee, no signature) and decodes the result to a native JS value. Used for
 * all read-only routes (`/quote`, pool stats) so the API never needs a
 * funded signing account of its own.
 */
export async function callReadOnly(contractId, method, args = []) {
  const contract = new Contract(contractId);
  const account = new Account(SIMULATION_SOURCE, '0');
  const tx = new TransactionBuilder(account, {
    fee: '100',
    networkPassphrase: config.networkPassphrase,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();

  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) {
    throw new Error(`Soroban simulation failed for ${contractId}.${method}: ${sim.error}`);
  }
  return scValToNative(sim.result.retval);
}

export { server };
