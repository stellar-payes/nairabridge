-- Pools known to the indexer/API. Seeded from contracts/scripts/deploy.sh
-- output (see src/db/seed-pools.js) since there is no on-chain pool
-- registry event stream to discover them from automatically until the
-- router's `register_pool` calls are also indexed.
CREATE TABLE IF NOT EXISTS pools (
  id TEXT PRIMARY KEY,                 -- pool contract address (strkey)
  tokens TEXT[] NOT NULL,              -- SEP-41 token contract addresses, pool order
  token_symbols TEXT[] NOT NULL,       -- human-readable, same order as tokens
  lp_token TEXT NOT NULL,
  amp NUMERIC NOT NULL,
  fee_bps INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS swaps (
  id BIGSERIAL PRIMARY KEY,
  pool_id TEXT NOT NULL REFERENCES pools(id),
  account TEXT NOT NULL,
  token_in TEXT NOT NULL,
  token_out TEXT NOT NULL,
  amount_in NUMERIC NOT NULL,
  amount_out NUMERIC NOT NULL,
  ledger_sequence BIGINT NOT NULL,
  tx_hash TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (tx_hash, pool_id, ledger_sequence)
);
CREATE INDEX IF NOT EXISTS idx_swaps_pool_created ON swaps (pool_id, created_at DESC);

CREATE TABLE IF NOT EXISTS liquidity_events (
  id BIGSERIAL PRIMARY KEY,
  pool_id TEXT NOT NULL REFERENCES pools(id),
  account TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('add', 'remove')),
  amounts NUMERIC[] NOT NULL,
  lp_amount NUMERIC NOT NULL,
  ledger_sequence BIGINT NOT NULL,
  tx_hash TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (tx_hash, pool_id, ledger_sequence)
);
CREATE INDEX IF NOT EXISTS idx_liquidity_events_pool_created ON liquidity_events (pool_id, created_at DESC);

-- 5-minute OHLC of the implied token_in/token_out price, built from swap
-- events as the indexer processes them.
CREATE TABLE IF NOT EXISTS pool_snapshots (
  id BIGSERIAL PRIMARY KEY,
  pool_id TEXT NOT NULL REFERENCES pools(id),
  token_in TEXT NOT NULL,
  token_out TEXT NOT NULL,
  bucket_start TIMESTAMPTZ NOT NULL,
  open NUMERIC NOT NULL,
  high NUMERIC NOT NULL,
  low NUMERIC NOT NULL,
  close NUMERIC NOT NULL,
  volume NUMERIC NOT NULL DEFAULT 0,
  UNIQUE (pool_id, token_in, token_out, bucket_start)
);
CREATE INDEX IF NOT EXISTS idx_snapshots_lookup
  ON pool_snapshots (pool_id, token_in, token_out, bucket_start DESC);

-- Indexer cursor per pool so restarts resume from the last processed
-- ledger instead of re-scanning from genesis.
CREATE TABLE IF NOT EXISTS indexer_state (
  pool_id TEXT PRIMARY KEY REFERENCES pools(id),
  last_ledger BIGINT NOT NULL DEFAULT 0,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
