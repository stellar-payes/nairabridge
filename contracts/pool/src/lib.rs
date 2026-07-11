#![no_std]

mod lp_token_client;

use lp_token_client::LpTokenClient;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, log, token, Address, Env, Symbol, Vec,
};

/// Basis points denominator (100.00%).
const BPS: i128 = 10_000;
/// Max swap/withdraw fee: 1%. Keeps a misconfigured `initialize` call from
/// bricking the pool into an unusable/predatory fee.
const MAX_FEE_BPS: u32 = 100;

#[derive(Clone, Copy)]
#[contracttype]
pub enum DataKey {
    Tokens,
    Reserves,
    LpToken,
    Admin,
    FeeBps,
    AdminFeeBps,
    AmpInitial,
    AmpTarget,
    RampStartTime,
    RampEndTime,
    Initialized,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PoolError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    LpTokenNotSet = 3,
    TooFewTokens = 4,
    TooManyTokens = 5,
    AmountsLengthMismatch = 6,
    FeeTooHigh = 7,
    InvalidAmp = 8,
    ZeroAmount = 9,
    TokenNotInPool = 10,
    SameToken = 11,
    SlippageExceeded = 12,
    MathError = 13,
    NoLiquidity = 14,
    RampEndInPast = 15,
}

#[contract]
pub struct Pool;

#[contractimpl]
impl Pool {
    /// One-time setup, called by the factory immediately after deploy.
    /// `tokens` must be 2 or 3 SEP-41 addresses (see
    /// `stableswap_math::MAX_COINS`) all sharing the same number of
    /// decimals — the caller is responsible for only pairing like-scaled
    /// assets (USDC/NGNC/EURC all use 7 decimals on Stellar).
    pub fn initialize(
        e: Env,
        tokens: Vec<Address>,
        amp: u128,
        fee_bps: u32,
        admin: Address,
    ) -> Result<(), PoolError> {
        if e.storage().instance().has(&DataKey::Initialized) {
            return Err(PoolError::AlreadyInitialized);
        }
        if tokens.len() < 2 {
            return Err(PoolError::TooFewTokens);
        }
        if tokens.len() as usize > stableswap_math::MAX_COINS {
            return Err(PoolError::TooManyTokens);
        }
        if fee_bps > MAX_FEE_BPS {
            return Err(PoolError::FeeTooHigh);
        }
        if amp == 0 {
            return Err(PoolError::InvalidAmp);
        }

        admin.require_auth();

        let mut reserves: Vec<i128> = Vec::new(&e);
        for _ in 0..tokens.len() {
            reserves.push_back(0);
        }

        e.storage().instance().set(&DataKey::Tokens, &tokens);
        e.storage().instance().set(&DataKey::Reserves, &reserves);
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        e.storage().instance().set(&DataKey::AdminFeeBps, &0u32);
        e.storage().instance().set(&DataKey::AmpInitial, &amp);
        e.storage().instance().set(&DataKey::AmpTarget, &amp);
        e.storage()
            .instance()
            .set(&DataKey::RampStartTime, &e.ledger().timestamp());
        e.storage()
            .instance()
            .set(&DataKey::RampEndTime, &e.ledger().timestamp());
        e.storage().instance().set(&DataKey::Initialized, &true);

        Ok(())
    }

    /// Registers the LP token contract for this pool. Called once by the
    /// factory after it deploys the `lp-token` contract for this pool (the
    /// LP token's own constructor needs the pool's address as its minter,
    /// creating a circular dependency that a single `initialize` call can't
    /// express).
    pub fn set_lp_token(e: Env, lp_token: Address) -> Result<(), PoolError> {
        Self::require_admin(&e)?;
        e.storage().instance().set(&DataKey::LpToken, &lp_token);
        Ok(())
    }

    pub fn add_liquidity(
        e: Env,
        from: Address,
        amounts: Vec<i128>,
        min_lp: i128,
    ) -> Result<i128, PoolError> {
        from.require_auth();
        Self::require_initialized(&e)?;

        let tokens: Vec<Address> = e.storage().instance().get(&DataKey::Tokens).unwrap();
        let mut reserves: Vec<i128> = e.storage().instance().get(&DataKey::Reserves).unwrap();
        if amounts.len() != tokens.len() {
            return Err(PoolError::AmountsLengthMismatch);
        }

        let amp = Self::current_amp(&e);
        let old_reserves_arr = to_array(&reserves);
        let d0 = if old_reserves_arr.iter().all(|&x| x == 0) {
            0
        } else {
            stableswap_math::get_d(&old_reserves_arr[..reserves.len() as usize], amp)
                .ok_or(PoolError::MathError)?
        };

        for i in 0..tokens.len() {
            let amount = amounts.get(i).unwrap();
            if amount < 0 {
                return Err(PoolError::ZeroAmount);
            }
            if amount > 0 {
                let token_client = token::Client::new(&e, &tokens.get(i).unwrap());
                token_client.transfer(&from, &e.current_contract_address(), &amount);
            }
            let updated = reserves.get(i).unwrap() + amount;
            reserves.set(i, updated);
        }

        let new_reserves_arr = to_array(&reserves);
        let d1 = stableswap_math::get_d(&new_reserves_arr[..reserves.len() as usize], amp)
            .ok_or(PoolError::MathError)?;
        if d1 <= d0 {
            return Err(PoolError::ZeroAmount);
        }

        let lp_token_addr = Self::lp_token_address(&e)?;
        let lp_client = LpTokenClient::new(&e, &lp_token_addr);
        let total_supply = lp_client.total_supply();

        let mint_amount = if total_supply == 0 {
            d1 // bootstrap: initial LP supply equals the invariant itself, Curve-style
        } else {
            // Mint proportional to the invariant's growth so existing LPs
            // aren't diluted relative to the value they hold.
            (d1 - d0)
                .checked_mul(total_supply)
                .ok_or(PoolError::MathError)?
                / d0
        };
        if mint_amount < min_lp {
            return Err(PoolError::SlippageExceeded);
        }

        e.storage().instance().set(&DataKey::Reserves, &reserves);
        lp_client.mint(&from, &mint_amount);

        e.events().publish(
            (Symbol::new(&e, "add_liq"), from.clone()),
            (amounts, mint_amount),
        );

        Ok(mint_amount)
    }

    /// Burns `lp_amount` LP shares and returns each pool asset pro-rata.
    /// LP tokens are held in the caller's own wallet (this pool has no LP
    /// vault), so `from` must have already called `approve` on the LP
    /// token contract, authorizing this pool address to burn up to
    /// `lp_amount` on their behalf.
    pub fn remove_liquidity(
        e: Env,
        from: Address,
        lp_amount: i128,
        min_amounts: Vec<i128>,
    ) -> Result<Vec<i128>, PoolError> {
        from.require_auth();
        Self::require_initialized(&e)?;

        if lp_amount <= 0 {
            return Err(PoolError::ZeroAmount);
        }

        let tokens: Vec<Address> = e.storage().instance().get(&DataKey::Tokens).unwrap();
        let mut reserves: Vec<i128> = e.storage().instance().get(&DataKey::Reserves).unwrap();
        if min_amounts.len() != tokens.len() {
            return Err(PoolError::AmountsLengthMismatch);
        }

        let lp_token_addr = Self::lp_token_address(&e)?;
        let lp_client = LpTokenClient::new(&e, &lp_token_addr);
        let total_supply = lp_client.total_supply();
        if total_supply == 0 {
            return Err(PoolError::NoLiquidity);
        }

        let mut out_amounts: Vec<i128> = Vec::new(&e);
        for i in 0..tokens.len() {
            let reserve = reserves.get(i).unwrap();
            let amount = reserve
                .checked_mul(lp_amount)
                .ok_or(PoolError::MathError)?
                / total_supply;
            if amount < min_amounts.get(i).unwrap() {
                return Err(PoolError::SlippageExceeded);
            }
            reserves.set(i, reserve - amount);
            out_amounts.push_back(amount);
        }

        // Burn before external transfers (checks-effects-interactions).
        lp_client.burn_from(&e.current_contract_address(), &from, &lp_amount);
        e.storage().instance().set(&DataKey::Reserves, &reserves);

        for i in 0..tokens.len() {
            let amount = out_amounts.get(i).unwrap();
            if amount > 0 {
                let token_client = token::Client::new(&e, &tokens.get(i).unwrap());
                token_client.transfer(&e.current_contract_address(), &from, &amount);
            }
        }

        e.events().publish(
            (Symbol::new(&e, "rem_liq"), from.clone()),
            (out_amounts.clone(), lp_amount),
        );

        Ok(out_amounts)
    }

    pub fn swap(
        e: Env,
        from: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_out: i128,
    ) -> Result<i128, PoolError> {
        from.require_auth();
        Self::require_initialized(&e)?;

        if amount_in <= 0 {
            return Err(PoolError::ZeroAmount);
        }

        let (dy_after_fee, mut reserves, i, j) =
            Self::quote_internal(&e, &token_in, &token_out, amount_in)?;
        if dy_after_fee < min_out {
            return Err(PoolError::SlippageExceeded);
        }

        let in_client = token::Client::new(&e, &token_in);
        in_client.transfer(&from, &e.current_contract_address(), &amount_in);

        reserves.set(i, reserves.get(i).unwrap() + amount_in);
        reserves.set(j, reserves.get(j).unwrap() - dy_after_fee);
        e.storage().instance().set(&DataKey::Reserves, &reserves);

        let out_client = token::Client::new(&e, &token_out);
        out_client.transfer(&e.current_contract_address(), &from, &dy_after_fee);

        e.events().publish(
            (Symbol::new(&e, "swap"), from.clone()),
            (token_in, token_out, amount_in, dy_after_fee),
        );

        Ok(dy_after_fee)
    }

    /// Read-only quote: same math as `swap` without moving any funds.
    pub fn get_quote(
        e: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
    ) -> Result<i128, PoolError> {
        Self::require_initialized(&e)?;
        if amount_in <= 0 {
            return Err(PoolError::ZeroAmount);
        }
        let (dy_after_fee, _, _, _) = Self::quote_internal(&e, &token_in, &token_out, amount_in)?;
        Ok(dy_after_fee)
    }

    /// Begin a time-weighted change of the amplification coefficient,
    /// mirroring Curve's `ramp_A` — abrupt amp changes move the invariant
    /// sharply and can be exploited around large trades, so changes are
    /// linearly interpolated over `[now, ramp_end]` instead of applied
    /// instantly.
    pub fn ramp_amp(e: Env, target_amp: u128, ramp_end: u64) -> Result<(), PoolError> {
        Self::require_admin(&e)?;
        if target_amp == 0 {
            return Err(PoolError::InvalidAmp);
        }
        let now = e.ledger().timestamp();
        if ramp_end <= now {
            return Err(PoolError::RampEndInPast);
        }

        let current = Self::current_amp(&e);
        e.storage().instance().set(&DataKey::AmpInitial, &current);
        e.storage().instance().set(&DataKey::AmpTarget, &target_amp);
        e.storage().instance().set(&DataKey::RampStartTime, &now);
        e.storage().instance().set(&DataKey::RampEndTime, &ramp_end);

        log!(&e, "ramp_amp: {} -> {} by {}", current, target_amp, ramp_end);
        Ok(())
    }

    pub fn get_reserves(e: Env) -> Vec<i128> {
        e.storage().instance().get(&DataKey::Reserves).unwrap()
    }

    pub fn get_tokens(e: Env) -> Vec<Address> {
        e.storage().instance().get(&DataKey::Tokens).unwrap()
    }

    pub fn get_amp(e: Env) -> u128 {
        Self::current_amp(&e)
    }

    // -- internal helpers --------------------------------------------------

    fn require_initialized(e: &Env) -> Result<(), PoolError> {
        if !e.storage().instance().has(&DataKey::Initialized) {
            return Err(PoolError::NotInitialized);
        }
        Ok(())
    }

    fn require_admin(e: &Env) -> Result<(), PoolError> {
        Self::require_initialized(e)?;
        let admin: Address = e.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        Ok(())
    }

    fn lp_token_address(e: &Env) -> Result<Address, PoolError> {
        e.storage()
            .instance()
            .get(&DataKey::LpToken)
            .ok_or(PoolError::LpTokenNotSet)
    }

    /// Amplification coefficient at the current ledger time, linearly
    /// interpolated between the value recorded at the start of the most
    /// recent `ramp_amp` call and its target.
    fn current_amp(e: &Env) -> u128 {
        let now = e.ledger().timestamp();
        let ramp_end: u64 = e.storage().instance().get(&DataKey::RampEndTime).unwrap();
        let target: u128 = e.storage().instance().get(&DataKey::AmpTarget).unwrap();

        if now >= ramp_end {
            return target;
        }

        let start: u128 = e.storage().instance().get(&DataKey::AmpInitial).unwrap();
        let start_time: u64 = e
            .storage()
            .instance()
            .get(&DataKey::RampStartTime)
            .unwrap();

        let elapsed = (now - start_time) as u128;
        let duration = (ramp_end - start_time) as u128;
        if duration == 0 {
            return target;
        }

        if target > start {
            start + (target - start) * elapsed / duration
        } else {
            start - (start - target) * elapsed / duration
        }
    }

    fn quote_internal(
        e: &Env,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
    ) -> Result<(i128, Vec<i128>, u32, u32), PoolError> {
        if token_in == token_out {
            return Err(PoolError::SameToken);
        }
        let tokens: Vec<Address> = e.storage().instance().get(&DataKey::Tokens).unwrap();
        let reserves: Vec<i128> = e.storage().instance().get(&DataKey::Reserves).unwrap();
        let i = index_of(&tokens, token_in).ok_or(PoolError::TokenNotInPool)?;
        let j = index_of(&tokens, token_out).ok_or(PoolError::TokenNotInPool)?;

        let amp = Self::current_amp(e);
        let reserves_arr = to_array(&reserves);
        let dy = stableswap_math::swap_to(
            &reserves_arr[..reserves.len() as usize],
            amp,
            i as usize,
            j as usize,
            amount_in,
        )
        .ok_or(PoolError::MathError)?;

        let fee_bps: u32 = e.storage().instance().get(&DataKey::FeeBps).unwrap();
        let fee = dy * (fee_bps as i128) / BPS;
        let dy_after_fee = dy - fee;

        Ok((dy_after_fee, reserves, i, j))
    }
}

fn index_of(tokens: &Vec<Address>, token: &Address) -> Option<u32> {
    for i in 0..tokens.len() {
        if tokens.get(i).unwrap() == *token {
            return Some(i);
        }
    }
    None
}

fn to_array(reserves: &Vec<i128>) -> [i128; stableswap_math::MAX_COINS] {
    let mut arr = [0i128; stableswap_math::MAX_COINS];
    for i in 0..reserves.len() {
        arr[i as usize] = reserves.get(i).unwrap();
    }
    arr
}

#[cfg(test)]
mod test;
