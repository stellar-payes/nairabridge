//! SEP-41 compliant LP token. One instance is deployed per pool by the
//! `factory` contract; the pool address is set as the sole `minter` and is
//! the only caller allowed to `mint` or `burn_from` (used by
//! `add_liquidity`/`remove_liquidity` respectively). Everything else
//! (`transfer`, `approve`, `balance`, ...) behaves like a normal SEP-41
//! token so LP shares are freely transferable / usable as collateral
//! elsewhere.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol,
};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Balance(Address),
    Allowance(Address, Address),
    TotalSupply,
    Minter,
    Decimals,
    Name,
    Symbol,
    Initialized,
}

#[derive(Clone)]
#[contracttype]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TokenError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InsufficientBalance = 3,
    InsufficientAllowance = 4,
    NegativeAmount = 5,
    NotMinter = 6,
    AllowanceExpired = 7,
}

#[contract]
pub struct LpToken;

#[contractimpl]
impl LpToken {
    pub fn initialize(
        e: Env,
        minter: Address,
        decimals: u32,
        name: String,
        symbol: String,
    ) -> Result<(), TokenError> {
        if e.storage().instance().has(&DataKey::Initialized) {
            return Err(TokenError::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::Minter, &minter);
        e.storage().instance().set(&DataKey::Decimals, &decimals);
        e.storage().instance().set(&DataKey::Name, &name);
        e.storage().instance().set(&DataKey::Symbol, &symbol);
        e.storage().instance().set(&DataKey::TotalSupply, &0i128);
        e.storage().instance().set(&DataKey::Initialized, &true);
        Ok(())
    }

    /// Minter-only (the pool that owns this LP token).
    pub fn mint(e: Env, to: Address, amount: i128) -> Result<(), TokenError> {
        Self::require_minter(&e)?;
        check_non_negative(amount)?;

        let balance = Self::balance(e.clone(), to.clone());
        write_balance(&e, &to, balance + amount);

        let supply: i128 = e.storage().instance().get(&DataKey::TotalSupply).unwrap();
        e.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(supply + amount));

        e.events()
            .publish((Symbol::new(&e, "mint"), to), amount);
        Ok(())
    }

    pub fn burn(e: Env, from: Address, amount: i128) -> Result<(), TokenError> {
        from.require_auth();
        Self::do_burn(&e, &from, amount)
    }

    /// Minter-only third-party burn: the pool holds a prior `approve` from
    /// `from` and burns on their behalf (used by `remove_liquidity`).
    pub fn burn_from(
        e: Env,
        spender: Address,
        from: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        spender.require_auth();
        Self::require_minter(&e)?;
        Self::spend_allowance(&e, &from, &spender, amount)?;
        Self::do_burn(&e, &from, amount)
    }

    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) -> Result<(), TokenError> {
        from.require_auth();
        Self::do_transfer(&e, &from, &to, amount)
    }

    pub fn transfer_from(
        e: Env,
        spender: Address,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        spender.require_auth();
        Self::spend_allowance(&e, &from, &spender, amount)?;
        Self::do_transfer(&e, &from, &to, amount)
    }

    pub fn approve(
        e: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) -> Result<(), TokenError> {
        from.require_auth();
        check_non_negative(amount)?;
        e.storage().temporary().set(
            &DataKey::Allowance(from.clone(), spender.clone()),
            &AllowanceValue {
                amount,
                expiration_ledger,
            },
        );
        e.events().publish(
            (Symbol::new(&e, "approve"), from, spender),
            (amount, expiration_ledger),
        );
        Ok(())
    }

    pub fn allowance(e: Env, from: Address, spender: Address) -> i128 {
        e.storage()
            .temporary()
            .get::<_, AllowanceValue>(&DataKey::Allowance(from, spender))
            .map(|a| {
                if a.expiration_ledger < e.ledger().sequence() {
                    0
                } else {
                    a.amount
                }
            })
            .unwrap_or(0)
    }

    pub fn balance(e: Env, id: Address) -> i128 {
        e.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn total_supply(e: Env) -> i128 {
        e.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn decimals(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::Decimals).unwrap()
    }

    pub fn name(e: Env) -> String {
        e.storage().instance().get(&DataKey::Name).unwrap()
    }

    pub fn symbol(e: Env) -> String {
        e.storage().instance().get(&DataKey::Symbol).unwrap()
    }

    // -- internal helpers --------------------------------------------------

    fn require_minter(e: &Env) -> Result<(), TokenError> {
        let minter: Address = e
            .storage()
            .instance()
            .get(&DataKey::Minter)
            .ok_or(TokenError::NotInitialized)?;
        minter.require_auth();
        Ok(())
    }

    fn do_transfer(e: &Env, from: &Address, to: &Address, amount: i128) -> Result<(), TokenError> {
        check_non_negative(amount)?;
        let from_balance = Self::balance(e.clone(), from.clone());
        if from_balance < amount {
            return Err(TokenError::InsufficientBalance);
        }
        write_balance(e, from, from_balance - amount);
        let to_balance = Self::balance(e.clone(), to.clone());
        write_balance(e, to, to_balance + amount);
        e.events()
            .publish((Symbol::new(e, "transfer"), from.clone(), to.clone()), amount);
        Ok(())
    }

    fn do_burn(e: &Env, from: &Address, amount: i128) -> Result<(), TokenError> {
        check_non_negative(amount)?;
        let balance = Self::balance(e.clone(), from.clone());
        if balance < amount {
            return Err(TokenError::InsufficientBalance);
        }
        write_balance(e, from, balance - amount);
        let supply: i128 = e.storage().instance().get(&DataKey::TotalSupply).unwrap();
        e.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(supply - amount));
        e.events()
            .publish((Symbol::new(e, "burn"), from.clone()), amount);
        Ok(())
    }

    fn spend_allowance(
        e: &Env,
        from: &Address,
        spender: &Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        let key = DataKey::Allowance(from.clone(), spender.clone());
        let allowance = e
            .storage()
            .temporary()
            .get::<_, AllowanceValue>(&key)
            .unwrap_or(AllowanceValue {
                amount: 0,
                expiration_ledger: 0,
            });
        if allowance.expiration_ledger < e.ledger().sequence() && allowance.amount > 0 {
            return Err(TokenError::AllowanceExpired);
        }
        if allowance.amount < amount {
            return Err(TokenError::InsufficientAllowance);
        }
        e.storage().temporary().set(
            &key,
            &AllowanceValue {
                amount: allowance.amount - amount,
                expiration_ledger: allowance.expiration_ledger,
            },
        );
        Ok(())
    }
}

fn check_non_negative(amount: i128) -> Result<(), TokenError> {
    if amount < 0 {
        return Err(TokenError::NegativeAmount);
    }
    Ok(())
}

fn write_balance(e: &Env, id: &Address, amount: i128) {
    e.storage()
        .persistent()
        .set(&DataKey::Balance(id.clone()), &amount);
}

#[cfg(test)]
mod test;
