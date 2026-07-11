import { query } from '../db/index.js';
import { callReadOnly } from '../soroban/client.js';

const DECIMALS = 1e7; // 7 decimals, matches Soroban SEP-41 default

function formatPool(row) {
  return {
    id: row.id,
    tokens: row.tokens,
    tokenSymbols: row.token_symbols,
    lpToken: row.lp_token,
    amp: Number(row.amp),
    feeBps: row.fee_bps,
  };
}

export default async function poolsRoutes(fastify) {
  fastify.get('/pools', async () => {
    const { rows } = await query('SELECT * FROM pools ORDER BY created_at');
    return rows.map(formatPool);
  });

  fastify.get('/pools/:id/stats', async (request, reply) => {
    const { id } = request.params;
    const { rows } = await query('SELECT * FROM pools WHERE id = $1', [id]);
    if (rows.length === 0) {
      return reply.code(404).send({ error: 'pool not found' });
    }
    const poolRow = rows[0];

    const reserves = await callReadOnly(id, 'get_reserves', []);
    // TVL assumes every pool asset is worth ~$1 -- true for the
    // USDC/NGNC/EURC stable pairs this DEX targets, not a general oracle.
    const tvl = reserves.reduce((sum, r) => sum + Number(r) / DECIMALS, 0);

    const { rows: volumeRows } = await query(
      `SELECT COALESCE(SUM(amount_in), 0) AS volume
       FROM swaps WHERE pool_id = $1 AND created_at > now() - interval '24 hours'`,
      [id]
    );
    const volume24h = Number(volumeRows[0].volume) / DECIMALS;
    const feeRevenue24h = volume24h * (poolRow.fee_bps / 10_000);
    const apy = tvl > 0 ? (feeRevenue24h / tvl) * 365 * 100 : 0;

    return {
      ...formatPool(poolRow),
      reserves: reserves.map((r) => Number(r) / DECIMALS),
      tvl,
      volume24h,
      apy,
    };
  });
}
