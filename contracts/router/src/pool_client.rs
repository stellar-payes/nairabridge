//! Client surface for the sibling `pool` contract, described without a
//! Cargo path-dependency so the router can call any deployed pool address
//! without linking pool's wasm into its own build (see the equivalent note
//! in `pool/src/lp_token_client.rs`).

use soroban_sdk::{contractclient, Address, Env, Vec};

#[contractclient(name = "PoolClient")]
pub trait PoolTrait {
    fn swap(
        env: Env,
        from: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
    ) -> i128;
    fn get_quote(env: Env, token_in: Address, token_out: Address, amount_in: i128) -> i128;
    fn get_tokens(env: Env) -> Vec<Address>;
}
