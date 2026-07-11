use crate::{LpToken, LpTokenClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup<'a>() -> (Env, LpTokenClient<'a>, Address, Address, Address) {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, LpToken);
    let client = LpTokenClient::new(&e, &contract_id);

    let minter = Address::generate(&e); // stands in for the pool contract
    let alice = Address::generate(&e);
    let bob = Address::generate(&e);

    client.initialize(
        &minter,
        &7,
        &String::from_str(&e, "NairaBridge USDC-NGNC LP"),
        &String::from_str(&e, "nbUSDC-NGNC"),
    );

    (e, client, minter, alice, bob)
}

#[test]
fn mint_increases_balance_and_supply() {
    let (_e, client, minter, alice, _bob) = setup();
    client.mint(&alice, &1_000);
    assert_eq!(client.balance(&alice), 1_000);
    assert_eq!(client.total_supply(), 1_000);
    let _ = minter; // minter identity asserted via require_auth inside mint
}

#[test]
fn transfer_moves_balance_between_accounts() {
    let (_e, client, _minter, alice, bob) = setup();
    client.mint(&alice, &500);
    client.transfer(&alice, &bob, &200);
    assert_eq!(client.balance(&alice), 300);
    assert_eq!(client.balance(&bob), 200);
}

#[test]
fn transfer_from_respects_and_decrements_allowance() {
    let (e, client, minter, alice, bob) = setup();
    client.mint(&alice, &1_000);
    client.approve(&alice, &minter, &400, &(e.ledger().sequence() + 100));

    client.transfer_from(&minter, &alice, &bob, &300);
    assert_eq!(client.balance(&alice), 700);
    assert_eq!(client.balance(&bob), 300);
    assert_eq!(client.allowance(&alice, &minter), 100);
}

#[test]
fn burn_decreases_balance_and_supply() {
    let (_e, client, _minter, alice, _bob) = setup();
    client.mint(&alice, &1_000);
    client.burn(&alice, &400);
    assert_eq!(client.balance(&alice), 600);
    assert_eq!(client.total_supply(), 600);
}

#[test]
fn burn_from_used_by_pool_on_remove_liquidity() {
    let (e, client, minter, alice, _bob) = setup();
    client.mint(&alice, &1_000);
    // Alice approves the pool (minter) to burn her LP shares on withdrawal.
    client.approve(&alice, &minter, &1_000, &(e.ledger().sequence() + 100));

    client.burn_from(&minter, &alice, &600);
    assert_eq!(client.balance(&alice), 400);
    assert_eq!(client.total_supply(), 400);
    assert_eq!(client.allowance(&alice, &minter), 400);
}

#[test]
fn transfer_from_fails_without_sufficient_allowance() {
    let (e, client, _minter, alice, bob) = setup();
    client.mint(&alice, &100);
    client.approve(&alice, &bob, &50, &(e.ledger().sequence() + 100));

    let result = client.try_transfer_from(&bob, &alice, &bob, &51);
    assert!(result.is_err());
}

#[test]
fn transfer_fails_on_insufficient_balance() {
    let (_e, client, _minter, alice, bob) = setup();
    client.mint(&alice, &10);
    let result = client.try_transfer(&alice, &bob, &11);
    assert!(result.is_err());
}
