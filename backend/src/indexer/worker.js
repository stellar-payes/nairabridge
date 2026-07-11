// Polls each known pool contract for `swap`/`add_liq`/`rem_liq` events,
// persists them, and rolls swaps up into 5-minute OHLC price snapshots.
// Runs as its own process (`npm run indexer`), separate from the API, so a
// slow RPC node never blocks request handling.
import { rpc, scValToNative } from '@stellar/stellar-sdk';
import { config } from '../config.js';
import { query } from '../db/index.js';

const server = new rpc.Server(config.sorobanRpcUrl, {
  allowHttp: config.sorobanRpcUrl.startsWith('http://'),
});

const POLL_INTERVAL_MS = 5_000;
const SNAPSHOT_BUCKET_MS = 5 * 60 * 1000;
const DECIMALS = 1e7;

async function getPoolIds() {
  const { rows } = await query('SELECT id FROM pools');
  return rows.map((r) => r.id);
}

async function getCursor(poolId) {
  const { rows } = await query('SELECT last_ledger FROM indexer_state WHERE pool_id = $1', [
    poolId,
  ]);
  if (rows.length > 0 && rows[0].last_ledger > 0) {
    return rows[0].last_ledger;
  }
  const latest = await server.getLatestLedger();
  // RPC only retains a rolling window of ledgers (~7 days), so on first
  // run there's nothing further back to backfill from anyway.
  return Math.max(1, latest.sequence - 1000);
}

async function setCursor(poolId, ledger) {
  await query(
    `INSERT INTO indexer_state (pool_id, last_ledger, updated_at) VALUES ($1, $2, now())
     ON CONFLICT (pool_id) DO UPDATE SET last_ledger = EXCLUDED.last_ledger, updated_at = now()`,
    [poolId, ledger]
  );
}

function bucketStart(closeTimeMs) {
  return new Date(Math.floor(closeTimeMs / SNAPSHOT_BUCKET_MS) * SNAPSHOT_BUCKET_MS);
}

async function upsertSnapshot(poolId, tokenIn, tokenOut, price, volume, closeTimeMs) {
  const bucket = bucketStart(closeTimeMs);
  await query(
    `INSERT INTO pool_snapshots (pool_id, token_in, token_out, bucket_start, open, high, low, close, volume)
     VALUES ($1, $2, $3, $4, $5, $5, $5, $5, $6)
     ON CONFLICT (pool_id, token_in, token_out, bucket_start) DO UPDATE SET
       high = GREATEST(pool_snapshots.high, EXCLUDED.high),
       low = LEAST(pool_snapshots.low, EXCLUDED.low),
       close = EXCLUDED.close,
       volume = pool_snapshots.volume + EXCLUDED.volume`,
    [poolId, tokenIn, tokenOut, bucket, price, volume]
  );
}

async function processSwapEvent(poolId, event) {
  const [, account] = event.topic.map(scValToNative); // topics: [Symbol("swap"), from]
  const [tokenIn, tokenOut, amountIn, amountOut] = scValToNative(event.value);
  const closeTimeMs = event.ledgerClosedAt ? Date.parse(event.ledgerClosedAt) : Date.now();

  await query(
    `INSERT INTO swaps (pool_id, account, token_in, token_out, amount_in, amount_out, ledger_sequence, tx_hash)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
     ON CONFLICT (tx_hash, pool_id, ledger_sequence) DO NOTHING`,
    [
      poolId,
      account,
      tokenIn,
      tokenOut,
      amountIn.toString(),
      amountOut.toString(),
      event.ledger,
      event.txHash,
    ]
  );

  const price = Number(amountOut) / Number(amountIn);
  await upsertSnapshot(poolId, tokenIn, tokenOut, price, Number(amountIn) / DECIMALS, closeTimeMs);
}

async function processLiquidityEvent(poolId, event, kind) {
  const [, account] = event.topic.map(scValToNative);
  const [amounts, lpAmount] = scValToNative(event.value);

  await query(
    `INSERT INTO liquidity_events (pool_id, account, kind, amounts, lp_amount, ledger_sequence, tx_hash)
     VALUES ($1, $2, $3, $4, $5, $6, $7)
     ON CONFLICT (tx_hash, pool_id, ledger_sequence) DO NOTHING`,
    [
      poolId,
      account,
      kind,
      amounts.map((a) => a.toString()),
      lpAmount.toString(),
      event.ledger,
      event.txHash,
    ]
  );
}

async function pollPool(poolId) {
  const startLedger = await getCursor(poolId);
  const response = await server.getEvents({
    startLedger,
    filters: [{ type: 'contract', contractIds: [poolId] }],
    limit: 100,
  });

  for (const event of response.events) {
    const [topicSymbol] = event.topic.map(scValToNative);
    if (topicSymbol === 'swap') {
      await processSwapEvent(poolId, event);
    } else if (topicSymbol === 'add_liq') {
      await processLiquidityEvent(poolId, event, 'add');
    } else if (topicSymbol === 'rem_liq') {
      await processLiquidityEvent(poolId, event, 'remove');
    }
  }

  if (response.events.length > 0) {
    const lastLedger = response.events[response.events.length - 1].ledger;
    await setCursor(poolId, lastLedger + 1);
  }
}

async function tick() {
  const poolIds = await getPoolIds();
  if (poolIds.length === 0) {
    console.warn('[indexer] no pools registered -- run `npm run migrate && node src/db/seed-pools.js` first');
  }
  for (const poolId of poolIds) {
    try {
      await pollPool(poolId);
    } catch (err) {
      console.error(`[indexer] pool ${poolId} failed:`, err.message);
    }
  }
}

async function main() {
  console.log(`[indexer] starting, polling every ${POLL_INTERVAL_MS}ms`);
  for (;;) {
    await tick();
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
}

main().catch((err) => {
  console.error('[indexer] fatal:', err);
  process.exit(1);
});
