//! Client surfaces for the `pool` and `lp-token` contracts the factory
//! deploys and wires together. Described via `#[contractclient]` rather
//! than a Cargo dependency for the same reason as the equivalent files in
//! `pool` and `router` — the factory only needs to *call* deployed
//! instances, not link their wasm into its own.

use soroban_sdk::{contractclient, Address, Env, String, Vec};

#[contractclient(name = "PoolClient")]
pub trait PoolTrait {
    fn initialize(env: Env, tokens: Vec<Address>, amp: u128, fee_bps: u32, admin: Address);
    fn set_lp_token(env: Env, lp_token: Address);
    fn ramp_amp(env: Env, target_amp: u128, ramp_end: u64);
}

#[contractclient(name = "LpTokenClient")]
pub trait LpTokenTrait {
    fn initialize(env: Env, minter: Address, decimals: u32, name: String, symbol: String);
}
