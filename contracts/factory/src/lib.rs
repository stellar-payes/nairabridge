//! Deploys and registers new pools. Each `deploy_pool` call installs a
//! fresh `pool` instance and a fresh `lp-token` instance from pre-uploaded
//! wasm, wires the LP token's minter to the new pool, and keeps the
//! factory's own address as that pool's on-chain admin — so day-to-day
//! governance (amp ramps) flows through `ramp_pool_amp` here, gated by the
//! factory's single human/multisig `admin`, rather than each pool having
//! its own independent admin key to manage.
#![no_std]

mod clients;

use clients::{LpTokenClient, PoolClient};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env, String,
    Symbol, Vec,
};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    PoolWasmHash,
    LpTokenWasmHash,
    Nonce,
    Pools,
    PoolByPair(Address, Address),
    Initialized,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FactoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    TooFewTokens = 3,
    TooManyTokens = 4,
    PairAlreadyExists = 5,
}

#[contract]
pub struct Factory;

#[contractimpl]
impl Factory {
    pub fn initialize(
        e: Env,
        admin: Address,
        pool_wasm_hash: BytesN<32>,
        lp_token_wasm_hash: BytesN<32>,
    ) -> Result<(), FactoryError> {
        if e.storage().instance().has(&DataKey::Initialized) {
            return Err(FactoryError::AlreadyInitialized);
        }
        admin.require_auth();

        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage()
            .instance()
            .set(&DataKey::PoolWasmHash, &pool_wasm_hash);
        e.storage()
            .instance()
            .set(&DataKey::LpTokenWasmHash, &lp_token_wasm_hash);
        e.storage().instance().set(&DataKey::Nonce, &0u32);
        e.storage()
            .instance()
            .set(&DataKey::Pools, &Vec::<Address>::new(&e));
        e.storage().instance().set(&DataKey::Initialized, &true);
        Ok(())
    }

    /// Deploys a new pool + LP token pair and wires them together.
    /// `tokens` must be ordered by pair for `get_pool` lookups to be
    /// meaningful for the 2-asset case; tri-pools are only reachable via
    /// `all_pools`.
    pub fn deploy_pool(
        e: Env,
        tokens: Vec<Address>,
        amp: u128,
        fee_bps: u32,
        lp_name: String,
        lp_symbol: String,
    ) -> Result<Address, FactoryError> {
        Self::require_admin(&e)?;
        if tokens.len() < 2 {
            return Err(FactoryError::TooFewTokens);
        }
        if tokens.len() as usize > stableswap_math::MAX_COINS {
            return Err(FactoryError::TooManyTokens);
        }
        if tokens.len() == 2
            && Self::get_pool(e.clone(), tokens.get(0).unwrap(), tokens.get(1).unwrap()).is_some()
        {
            return Err(FactoryError::PairAlreadyExists);
        }

        let factory_address = e.current_contract_address();

        let pool_wasm_hash: BytesN<32> = e.storage().instance().get(&DataKey::PoolWasmHash).unwrap();
        let lp_wasm_hash: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::LpTokenWasmHash)
            .unwrap();

        let pool_address = e
            .deployer()
            .with_current_contract(Self::next_salt(&e))
            .deploy(pool_wasm_hash);
        let lp_address = e
            .deployer()
            .with_current_contract(Self::next_salt(&e))
            .deploy(lp_wasm_hash);

        // The factory is the pool's on-chain admin (see module docs); this
        // self-authorizes because the factory is the direct caller.
        PoolClient::new(&e, &pool_address).initialize(
            &tokens,
            &amp,
            &fee_bps,
            &factory_address,
        );
        LpTokenClient::new(&e, &lp_address).initialize(&pool_address, &7u32, &lp_name, &lp_symbol);
        PoolClient::new(&e, &pool_address).set_lp_token(&lp_address);

        let mut pools: Vec<Address> = e.storage().instance().get(&DataKey::Pools).unwrap();
        pools.push_back(pool_address.clone());
        e.storage().instance().set(&DataKey::Pools, &pools);

        if tokens.len() == 2 {
            let (a, b) = order(&tokens.get(0).unwrap(), &tokens.get(1).unwrap());
            e.storage()
                .instance()
                .set(&DataKey::PoolByPair(a, b), &pool_address);
        }

        e.events().publish(
            (Symbol::new(&e, "pool_deployed"), pool_address.clone()),
            (tokens, lp_address, amp, fee_bps),
        );

        Ok(pool_address)
    }

    /// Forwards an amp-ramp request to a pool this factory administers.
    pub fn ramp_pool_amp(
        e: Env,
        pool: Address,
        target_amp: u128,
        ramp_end: u64,
    ) -> Result<(), FactoryError> {
        Self::require_admin(&e)?;
        PoolClient::new(&e, &pool).ramp_amp(&target_amp, &ramp_end);
        Ok(())
    }

    pub fn get_pool(e: Env, token_a: Address, token_b: Address) -> Option<Address> {
        let (a, b) = order(&token_a, &token_b);
        e.storage().instance().get(&DataKey::PoolByPair(a, b))
    }

    pub fn all_pools(e: Env) -> Vec<Address> {
        e.storage()
            .instance()
            .get(&DataKey::Pools)
            .unwrap_or(Vec::new(&e))
    }

    fn require_admin(e: &Env) -> Result<(), FactoryError> {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FactoryError::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }

    fn next_salt(e: &Env) -> BytesN<32> {
        let nonce: u32 = e.storage().instance().get(&DataKey::Nonce).unwrap_or(0);
        e.storage().instance().set(&DataKey::Nonce, &(nonce + 1));

        let mut bytes = Bytes::new(e);
        bytes.extend_from_array(&nonce.to_be_bytes());
        e.crypto().sha256(&bytes).to_bytes()
    }
}

fn order(a: &Address, b: &Address) -> (Address, Address) {
    if a <= b {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}

#[cfg(test)]
mod test;
