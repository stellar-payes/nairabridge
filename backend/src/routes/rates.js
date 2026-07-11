import { query } from '../db/index.js';

const INTERVALS = {
  '5m': '5 minutes',
  '1h': '1 hour',
  '1d': '1 day',
};

async function resolveTokenAddress(symbol) {
  const { rows } = await query(
    `SELECT tokens[array_position(token_symbols, $1)] AS address
     FROM pools WHERE $1 = ANY(token_symbols) LIMIT 1`,
    [symbol]
  );
  return rows[0]?.address ?? null;
}

export default async function ratesRoutes(fastify) {
  fastify.get('/rates/history', async (request, reply) => {
    const { pair, interval = '1h' } = request.query;
    if (!pair || !pair.includes('-')) {
      return reply.code(400).send({ error: 'pair is required, e.g. USDC-NGNC' });
    }
    if (!INTERVALS[interval]) {
      return reply
        .code(400)
        .send({ error: `interval must be one of ${Object.keys(INTERVALS).join(', ')}` });
    }

    const [inSymbol, outSymbol] = pair.split('-');
    const [tokenIn, tokenOut] = await Promise.all([
      resolveTokenAddress(inSymbol),
      resolveTokenAddress(outSymbol),
    ]);
    if (!tokenIn || !tokenOut) {
      return reply.code(404).send({ error: 'unknown pair' });
    }

    const { rows } = await query(
      `SELECT
         date_bin($1::interval, bucket_start, TIMESTAMPTZ 'epoch') AS bucket,
         (array_agg(open ORDER BY bucket_start ASC))[1] AS open,
         MAX(high) AS high,
         MIN(low) AS low,
         (array_agg(close ORDER BY bucket_start DESC))[1] AS close,
         SUM(volume) AS volume
       FROM pool_snapshots
       WHERE token_in = $2 AND token_out = $3
       GROUP BY bucket
       ORDER BY bucket ASC`,
      [INTERVALS[interval], tokenIn, tokenOut]
    );

    return { pair, interval, candles: rows };
  });
}
