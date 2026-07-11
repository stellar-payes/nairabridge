#!/usr/bin/env bash
# Deploys the full NairaBridge contract set to Stellar testnet:
#   stableswap-math (linked in, not deployed on its own)
#   -> lp-token + pool wasm uploaded
#   -> factory deployed & initialized with those wasm hashes
#   -> factory.deploy_pool for each configured pair
#   -> router deployed, initialized, and registered against every pool
#
# Requires: stellar-cli (`cargo install --locked stellar-cli`), an identity
# named `nairabridge-admin` already funded on testnet
# (`stellar keys generate nairabridge-admin --network testnet --fund`).
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
ADMIN_IDENTITY="${ADMIN_IDENTITY:-nairabridge-admin}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WASM_DIR="$ROOT_DIR/target/wasm32v1-none/release"

# USDC/NGNC/EURC testnet issuer contract addresses (SAC-wrapped Stellar
# assets or third-party SEP-41 tokens) -- fill these in before running.
USDC_ADDR="${USDC_ADDR:?set USDC_ADDR to the testnet USDC SEP-41 contract address}"
NGNC_ADDR="${NGNC_ADDR:?set NGNC_ADDR to the testnet NGNC SEP-41 contract address}"
EURC_ADDR="${EURC_ADDR:?set EURC_ADDR to the testnet EURC SEP-41 contract address}"

AMP="${AMP:-100}"
FEE_BPS="${FEE_BPS:-30}" # 0.3%

echo "==> Building contracts (wasm32v1-none, release)"
cd "$ROOT_DIR"
stellar contract build

deploy_wasm_hash() {
  local wasm_path="$1"
  stellar contract upload \
    --wasm "$wasm_path" \
    --source "$ADMIN_IDENTITY" \
    --network "$NETWORK"
}

echo "==> Uploading pool and lp-token wasm"
POOL_WASM_HASH="$(deploy_wasm_hash "$WASM_DIR/pool.wasm")"
LP_TOKEN_WASM_HASH="$(deploy_wasm_hash "$WASM_DIR/lp_token.wasm")"
echo "    pool wasm hash:     $POOL_WASM_HASH"
echo "    lp-token wasm hash: $LP_TOKEN_WASM_HASH"

echo "==> Deploying factory"
FACTORY_ID="$(stellar contract deploy \
  --wasm "$WASM_DIR/factory.wasm" \
  --source "$ADMIN_IDENTITY" \
  --network "$NETWORK")"
echo "    factory: $FACTORY_ID"

ADMIN_ADDR="$(stellar keys address "$ADMIN_IDENTITY")"

stellar contract invoke \
  --id "$FACTORY_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
  -- initialize \
  --admin "$ADMIN_ADDR" \
  --pool_wasm_hash "$POOL_WASM_HASH" \
  --lp_token_wasm_hash "$LP_TOKEN_WASM_HASH"

deploy_pool() {
  local token_a="$1" token_b="$2" lp_name="$3" lp_symbol="$4"
  stellar contract invoke \
    --id "$FACTORY_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
    -- deploy_pool \
    --tokens "[\"$token_a\",\"$token_b\"]" \
    --amp "$AMP" \
    --fee_bps "$FEE_BPS" \
    --lp_name "$lp_name" \
    --lp_symbol "$lp_symbol"
}

echo "==> Deploying USDC/NGNC pool"
USDC_NGNC_POOL="$(deploy_pool "$USDC_ADDR" "$NGNC_ADDR" "NairaBridge USDC-NGNC LP" "nbUSDC-NGNC")"
echo "    pool: $USDC_NGNC_POOL"

echo "==> Deploying USDC/EURC pool"
USDC_EURC_POOL="$(deploy_pool "$USDC_ADDR" "$EURC_ADDR" "NairaBridge USDC-EURC LP" "nbUSDC-EURC")"
echo "    pool: $USDC_EURC_POOL"

echo "==> Deploying router"
ROUTER_ID="$(stellar contract deploy \
  --wasm "$WASM_DIR/router.wasm" \
  --source "$ADMIN_IDENTITY" \
  --network "$NETWORK")"
echo "    router: $ROUTER_ID"

stellar contract invoke \
  --id "$ROUTER_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
  -- initialize --admin "$ADMIN_ADDR"

register_with_router() {
  local token_a="$1" token_b="$2" pool="$3"
  stellar contract invoke \
    --id "$ROUTER_ID" --source "$ADMIN_IDENTITY" --network "$NETWORK" \
    -- register_pool --token_a "$token_a" --token_b "$token_b" --pool "$pool"
}

echo "==> Registering pools with router"
register_with_router "$USDC_ADDR" "$NGNC_ADDR" "$USDC_NGNC_POOL"
register_with_router "$USDC_ADDR" "$EURC_ADDR" "$USDC_EURC_POOL"

cat <<EOF

==> Deployment complete

FACTORY_ID=$FACTORY_ID
ROUTER_ID=$ROUTER_ID
USDC_NGNC_POOL=$USDC_NGNC_POOL
USDC_EURC_POOL=$USDC_EURC_POOL

Copy these into backend/.env and frontend/.env.local.
EOF
