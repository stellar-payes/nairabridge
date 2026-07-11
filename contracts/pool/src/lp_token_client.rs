//! Minimal client surface for the sibling `lp-token` contract.
//!
//! We deliberately avoid a Cargo path-dependency on the `lp-token` crate
//! (that would force pool and lp-token into the same wasm and couple their
//! deploys). Instead we describe the subset of the SEP-41 interface the
//! pool needs to drive, and `#[contractclient]` generates a typed client
//! that calls the *deployed* lp-token contract by address at runtime.

use soroban_sdk::{contractclient, Address, Env};

#[contractclient(name = "LpTokenClient")]
pub trait LpTokenTrait {
    fn mint(env: Env, to: Address, amount: i128);
    /// SEP-41 third-party burn: the pool holds a prior `approve` from
    /// `from` and burns on their behalf during `remove_liquidity`.
    fn burn_from(env: Env, spender: Address, from: Address, amount: i128);
    fn total_supply(env: Env) -> i128;
    fn balance(env: Env, id: Address) -> i128;
}
