# NairaBridge — Stablecoin Remittance DEX on Stellar

> Swap and send stablecoins (USDC ⇄ NGNC ⇄ EURC) for near-zero fees, built for everyday remittances, not speculation.

## Problem
Cross-border remittances to Africa cost 7–8% on average. People sending money home don't want charts and candles — they want "money in, naira out." Existing DEXes are trader-focused and give poor rates on stablecoin pairs because they use constant-product curves.

## Solution
A stable-swap AMM (Curve-style invariant) on Soroban, optimized for like-valued assets, wrapped in a "Send Money" UI that hides the swap entirely. Fiat on/off ramps via Stellar anchors (SEP-24 / SEP-31).

## Monorepo Structure
```
nairabridge/
├── contracts/
│   ├── stableswap-math/     # Reusable no_std Rust crate: invariant, D & y solvers (publish to crates.io)
│   ├── pool/                # Pool contract per asset pair/tri-pool
│   ├── lp-token/            # SEP-41 compliant LP token
│   ├── router/              # Multi-hop routing (USDC→XLM→NGNC)
│   └── factory/             # Deploys & registers pools
├── backend/                 # Node.js (Fastify) + Postgres
└── frontend/                # Next.js 14, mobile-first
```

## Contracts (Rust / Soroban SDK)
### pool
```rust
fn initialize(e: Env, tokens: Vec<Address>, amp: u128, fee_bps: u32, admin: Address);
fn add_liquidity(e: Env, from: Address, amounts: Vec<i128>, min_lp: i128) -> i128;
fn remove_liquidity(e: Env, from: Address, lp_amount: i128, min_amounts: Vec<i128>) -> Vec<i128>;
fn swap(e: Env, from: Address, token_in: Address, token_out: Address, amount_in: i128, min_out: i128) -> i128;
fn get_quote(e: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128;  // view
fn ramp_amp(e: Env, target_amp: u128, ramp_end: u64);  // admin, time-weighted like Curve
```
- Storage: `Reserves(Vec<i128>)`, `Amp`, `FeeBps`, `AdminFeeBps`, `LpToken(Address)`
- Events: `swap`, `add_liq`, `rem_liq` with amounts + addresses
- Invariant: StableSwap `A·n^n·Σx + D = A·D·n^n + D^(n+1)/(n^n·Πx)`; Newton's method solvers live in `stableswap-math` with full unit tests against Curve reference vectors.

### router
```rust
fn swap_exact_in(e: Env, from: Address, path: Vec<Address>, pools: Vec<Address>, amount_in: i128, min_out: i128) -> i128;
fn find_best_route(e: Env, token_in: Address, token_out: Address, amount_in: i128) -> (Vec<Address>, i128); // view
```

## Backend (Fastify + Postgres + soroban-rpc)
- **Indexer worker:** poll `getEvents` for pool contracts → tables `pools`, `swaps`, `liquidity_events`, `pool_snapshots` (5-min OHLC of implied price).
- **API:**
  - `GET /quote?in=USDC&out=NGNC&amount=100` → route, amountOut, priceImpact, feePaid
  - `GET /pools` / `GET /pools/:id/stats` (TVL, 24h volume, APY from fees)
  - `GET /rates/history?pair=USDC-NGNC&interval=1h`
  - `POST /anchor/deposit-url` → initiates SEP-24 interactive deposit, returns URL
- Env: `SOROBAN_RPC_URL`, `NETWORK_PASSPHRASE`, `DATABASE_URL`, `ANCHOR_HOME_DOMAINS`

## Frontend (Next.js + Tailwind + stellar-wallets-kit)
Pages:
1. **/send** — the hero flow: enter amount in USD, recipient Stellar address or federation `name*domain`, show "recipient gets ₦X" quote, one confirm button. Swap + transfer batched in one transaction.
2. **/swap** — classic swap UI with slippage control (default 0.5%).
3. **/pool** — LP dashboard: deposit, withdraw, earned fees, pool APY.
4. **/cashout** — SEP-24 anchor withdrawal (bank transfer).
- Wallets: Freighter, xBull, Albedo, Lobstr via `@creit.tech/stellar-wallets-kit`.
- All amounts displayed in local currency with `Intl.NumberFormat`.

## Milestones (label as GitHub issues)
1. `stableswap-math` crate with property tests (good-first-issue: add fuzz tests)
2. Pool contract + testnet deploy scripts (`scripts/deploy.sh` using stellar-cli)
3. Indexer + `/quote` API
4. Swap UI on testnet
5. Router + multi-hop
6. SEP-24 anchor integration (MoneyGram Access / local NGN anchor)
7. Audit prep: invariant checks, reentrancy review, admin timelock

## Getting Started
```bash
# contracts
cd contracts && stellar contract build && cargo test
# backend
cd backend && cp .env.example .env && npm i && npm run dev
# frontend
cd frontend && npm i && npm run dev
```

## FUNDING.json (repo root)
```json
{ "drips": { "ethereum": { "ownedBy": "0xYOUR_ADDRESS" } } }
```

## License
MIT — maximizes reuse and dependency adoption.
