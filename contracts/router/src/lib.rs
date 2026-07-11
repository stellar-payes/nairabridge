//! Multi-hop router sitting in front of the individual stable pools.
//!
//! Pools only know how to swap within their own token set (2–3 assets), so
//! routing "USDC → XLM → NGNC" across two separate pools needs a contract
//! that can hop through an intermediate ("hub") token. The router keeps a
//! small on-chain registry of `(token_a, token_b) -> pool` and a list of hub
//! tokens (populated by the `factory` as pools are deployed) so
//! `find_best_route` can compare a direct pool against every hub route and
//! `swap_exact_in` can execute whichever path the caller settled on.
#![no_std]

mod pool_client;

use pool_client::PoolClient;
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, token, Address, Env, Vec};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Hubs,
    Pool(Address, Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RouterError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    PathTooShort = 3,
    PoolCountMismatch = 4,
    ZeroAmount = 5,
    SlippageExceeded = 6,
    NoRouteFound = 7,
}

#[contract]
pub struct Router;

#[contractimpl]
impl Router {
    pub fn initialize(e: Env, admin: Address) -> Result<(), RouterError> {
        if e.storage().instance().has(&DataKey::Admin) {
            return Err(RouterError::AlreadyInitialized);
        }
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Hubs, &Vec::<Address>::new(&e));
        Ok(())
    }

    /// Registers a pool for direct (`token_a`, `token_b`) routing and,
    /// separately, tracks 2-and-3-way pools' constituent tokens as
    /// candidate hubs so future hops can route through them. Called by the
    /// `factory` immediately after it deploys a new pool.
    pub fn register_pool(
        e: Env,
        token_a: Address,
        token_b: Address,
        pool: Address,
    ) -> Result<(), RouterError> {
        Self::require_admin(&e)?;
        let (a, b) = order(&token_a, &token_b);
        e.storage()
            .instance()
            .set(&DataKey::Pool(a.clone(), b.clone()), &pool);

        let mut hubs: Vec<Address> = e.storage().instance().get(&DataKey::Hubs).unwrap();
        for t in [a, b] {
            if !hubs.iter().any(|h| h == t) {
                hubs.push_back(t);
            }
        }
        e.storage().instance().set(&DataKey::Hubs, &hubs);
        Ok(())
    }

    pub fn get_pool(e: Env, token_a: Address, token_b: Address) -> Option<Address> {
        let (a, b) = order(&token_a, &token_b);
        e.storage().instance().get(&DataKey::Pool(a, b))
    }

    /// Compares the direct pool (if any) against every single-hub route
    /// (`token_in -> hub -> token_out`) and returns whichever yields the
    /// larger output. Returns an empty path and `0` if no route exists.
    /// This is a read-only helper the frontend/backend calls to get a
    /// concrete `(path, pools)` pair before submitting `swap_exact_in`.
    pub fn find_best_route(
        e: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
    ) -> (Vec<Address>, Vec<Address>, i128) {
        let mut best_path: Vec<Address> = Vec::new(&e);
        let mut best_pools: Vec<Address> = Vec::new(&e);
        let mut best_out: i128 = 0;

        if let Some(direct_pool) = Self::get_pool(e.clone(), token_in.clone(), token_out.clone()) {
            let pool_client = PoolClient::new(&e, &direct_pool);
            let out = pool_client.get_quote(&token_in, &token_out, &amount_in);
            if out > best_out {
                best_out = out;
                best_path = Vec::from_array(&e, [token_in.clone(), token_out.clone()]);
                best_pools = Vec::from_array(&e, [direct_pool]);
            }
        }

        let hubs: Vec<Address> = e.storage().instance().get(&DataKey::Hubs).unwrap_or(Vec::new(&e));
        for hub in hubs.iter() {
            if hub == token_in || hub == token_out {
                continue;
            }
            let (Some(pool1), Some(pool2)) = (
                Self::get_pool(e.clone(), token_in.clone(), hub.clone()),
                Self::get_pool(e.clone(), hub.clone(), token_out.clone()),
            ) else {
                continue;
            };

            let leg1 = PoolClient::new(&e, &pool1).get_quote(&token_in, &hub, &amount_in);
            if leg1 <= 0 {
                continue;
            }
            let leg2 = PoolClient::new(&e, &pool2).get_quote(&hub, &token_out, &leg1);

            if leg2 > best_out {
                best_out = leg2;
                best_path = Vec::from_array(&e, [token_in.clone(), hub.clone(), token_out.clone()]);
                best_pools = Vec::from_array(&e, [pool1, pool2]);
            }
        }

        (best_path, best_pools, best_out)
    }

    /// Executes a swap along an explicit `path`/`pools` pair (typically
    /// obtained from `find_best_route`). Pulls `amount_in` of `path[0]`
    /// from `from` once, hops it through each pool while holding custody
    /// itself, then forwards the final output back to `from`.
    pub fn swap_exact_in(
        e: Env,
        from: Address,
        path: Vec<Address>,
        pools: Vec<Address>,
        amount_in: i128,
        min_out: i128,
    ) -> Result<i128, RouterError> {
        from.require_auth();
        if path.len() < 2 {
            return Err(RouterError::PathTooShort);
        }
        if pools.len() != path.len() - 1 {
            return Err(RouterError::PoolCountMismatch);
        }
        if amount_in <= 0 {
            return Err(RouterError::ZeroAmount);
        }

        let router_address = e.current_contract_address();
        let first_token = path.get(0).unwrap();
        token::Client::new(&e, &first_token).transfer(&from, &router_address, &amount_in);

        let mut current_amount = amount_in;
        for i in 0..pools.len() {
            let pool_addr = pools.get(i).unwrap();
            let token_in = path.get(i).unwrap();
            let token_out = path.get(i + 1).unwrap();

            // `from = router_address` here: the router is the direct
            // caller, so this self-authorizes without an extra user
            // signature (see pool_client.rs).
            current_amount = PoolClient::new(&e, &pool_addr).swap(
                &router_address,
                &token_in,
                &token_out,
                &current_amount,
                &0,
            );
        }

        if current_amount < min_out {
            return Err(RouterError::SlippageExceeded);
        }

        let last_token = path.get(path.len() - 1).unwrap();
        token::Client::new(&e, &last_token).transfer(&router_address, &from, &current_amount);

        Ok(current_amount)
    }

    fn require_admin(e: &Env) -> Result<(), RouterError> {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RouterError::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }
}

/// Deterministic ordering so `(a, b)` and `(b, a)` map to the same registry
/// slot regardless of call argument order.
fn order(a: &Address, b: &Address) -> (Address, Address) {
    if a <= b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}

#[cfg(test)]
mod test;
