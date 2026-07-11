// Upserts pool metadata from config/pools.json (copy config/pools.example.json
// and fill in the addresses printed by contracts/scripts/deploy.sh) so the
// indexer and API know which pool contracts to watch.
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { pool as db } from './index.js';

const configPath = fileURLToPath(new URL('../../config/pools.json', import.meta.url));

async function seed() {
  const raw = await readFile(configPath, 'utf8');
  const pools = JSON.parse(raw);

  for (const p of pools) {
    await db.query(
      `INSERT INTO pools (id, tokens, token_symbols, lp_token, amp, fee_bps)
       VALUES ($1, $2, $3, $4, $5, $6)
       ON CONFLICT (id) DO UPDATE SET
         tokens = EXCLUDED.tokens,
         token_symbols = EXCLUDED.token_symbols,
         lp_token = EXCLUDED.lp_token,
         amp = EXCLUDED.amp,
         fee_bps = EXCLUDED.fee_bps`,
      [p.id, p.tokens, p.tokenSymbols, p.lpToken, p.amp, p.feeBps]
    );
    await db.query(
      `INSERT INTO indexer_state (pool_id, last_ledger) VALUES ($1, 0)
       ON CONFLICT (pool_id) DO NOTHING`,
      [p.id]
    );
    console.log(`Seeded pool ${p.tokenSymbols.join('/')} (${p.id})`);
  }

  await db.end();
}

seed().catch((err) => {
  console.error('Seeding failed:', err);
  process.exit(1);
});
