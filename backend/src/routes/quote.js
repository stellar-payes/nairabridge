import { Address, nativeToScVal } from '@stellar/stellar-sdk';
import { config } from '../config.js';
import { query } from '../db/index.js';
import { callReadOnly } from '../soroban/client.js';

const DECIMALS = 10_000_000n; // 7 decimals, matches Soroban SEP-41 default

async function resolveTokenAddress(symbol) {
  const { rows } = await query(
    `SELECT tokens[array_position(token_symbols, $1)] AS address
     FROM pools WHERE $1 = ANY(token_symbols) LIMIT 1`,
    [symbol]
  );
  return rows[0]?.address ?? null;
}

export default async function quoteRoutes(fastify) {
  fastify.get('/quote', async (request, reply) => {
    const { in: inSymbol, out: outSymbol, amount } = request.query;
    if (!inSymbol || !outSymbol || !amount || Number.isNaN(Number(amount))) {
      return reply.code(400).send({ error: 'in, out, and amount query params are required' });
    }
    if (!config.routerContractId) {
      return reply.code(503).send({ error: 'ROUTER_CONTRACT_ID not configured' });
    }

    const [tokenIn, tokenOut] = await Promise.all([
      resolveTokenAddress(inSymbol),
      resolveTokenAddress(outSymbol),
    ]);
    if (!tokenIn || !tokenOut) {
      return reply.code(404).send({ error: 'unknown token symbol' });
    }

    const amountIn = BigInt(Math.round(Number(amount) * Number(DECIMALS)));

    const [path, pools, amountOut] = await callReadOnly(
      config.routerContractId,
      'find_best_route',
      [
        new Address(tokenIn).toScVal(),
        new Address(tokenOut).toScVal(),
        nativeToScVal(amountIn, { type: 'i128' }),
      ]
    );

    if (!path || path.length === 0) {
      return reply.code(404).send({ error: 'no route found for this pair' });
    }

    // The router only returns net output, not an itemized per-hop fee, so
    // this approximates combined fee + slippage as the deviation from a
    // perfect 1:1 stable swap -- good enough for a UI estimate, not for
    // accounting.
    const priceImpact = 1 - Number(amountOut) / Number(amountIn);

    return {
      route: path,
      pools,
      amountIn: Number(amountIn) / Number(DECIMALS),
      amountOut: Number(amountOut) / Number(DECIMALS),
      priceImpact,
    };
  });
}
