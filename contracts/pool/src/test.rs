use crate::{Pool, PoolClient};
use lp_token::{LpToken, LpTokenClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String, Vec,
};

const FEE_BPS: u32 = 30; // 0.3%
const AMP: u128 = 100;

struct TestFixture<'a> {
    e: Env,
    pool: PoolClient<'a>,
    lp: LpTokenClient<'a>,
    token_a: Address,
    token_b: Address,
    token_a_admin: token::StellarAssetClient<'a>,
    token_b_admin: token::StellarAssetClient<'a>,
    user: Address,
}

fn setup() -> TestFixture<'static> {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let user = Address::generate(&e);

    let token_a_sac = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_b_sac = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_a_admin = token::StellarAssetClient::new(&e, &token_a_sac);
    let token_b_admin = token::StellarAssetClient::new(&e, &token_b_sac);

    let pool_id = e.register_contract(None, Pool);
    let pool = PoolClient::new(&e, &pool_id);

    let mut tokens: Vec<Address> = Vec::new(&e);
    tokens.push_back(token_a_sac.clone());
    tokens.push_back(token_b_sac.clone());
    pool.initialize(&tokens, &AMP, &FEE_BPS, &admin);

    let lp_id = e.register_contract(None, LpToken);
    let lp = LpTokenClient::new(&e, &lp_id);
    lp.initialize(
        &pool_id,
        &7,
        &String::from_str(&e, "NairaBridge USDC-NGNC LP"),
        &String::from_str(&e, "nbLP"),
    );
    pool.set_lp_token(&lp_id);

    // 1,000,000 units of each asset (7 decimals) for the test user -- generous
    // headroom so tests can both seed pool liquidity and still swap afterward.
    token_a_admin.mint(&user, &10_000_000_000_000);
    token_b_admin.mint(&user, &10_000_000_000_000);

    TestFixture {
        e,
        pool,
        lp,
        token_a: token_a_sac,
        token_b: token_b_sac,
        token_a_admin,
        token_b_admin,
        user,
    }
}

const UNIT: i128 = 10_000_000; // 7 decimals

#[test]
fn add_liquidity_bootstraps_pool_and_mints_lp_1_to_1_with_d() {
    let f = setup();
    let amounts = Vec::from_array(&f.e, [100 * UNIT, 100 * UNIT]);

    let minted = f.pool.add_liquidity(&f.user, &amounts, &0);

    assert_eq!(minted, 200 * UNIT); // balanced bootstrap: LP == D == sum(x)
    assert_eq!(f.lp.balance(&f.user), minted);
    assert_eq!(f.lp.total_supply(), minted);
    assert_eq!(
        f.pool.get_reserves(),
        Vec::from_array(&f.e, [100 * UNIT, 100 * UNIT])
    );
}

#[test]
fn add_liquidity_after_bootstrap_mints_proportionally() {
    let f = setup();
    let amounts = Vec::from_array(&f.e, [100 * UNIT, 100 * UNIT]);
    f.pool.add_liquidity(&f.user, &amounts, &0);
    let supply_before = f.lp.total_supply();

    // Second, smaller balanced deposit should mint proportionally.
    let second = Vec::from_array(&f.e, [10 * UNIT, 10 * UNIT]);
    let minted = f.pool.add_liquidity(&f.user, &second, &0);

    // 10% of the pool added -> ~10% of supply minted (exact for a balanced,
    // fee-free deposit since D grows linearly with a balanced add).
    let expected = supply_before / 10;
    assert!((minted - expected).abs() <= 1);
}

#[test]
fn swap_near_peg_has_low_slippage_and_respects_fee() {
    let f = setup();
    let amounts = Vec::from_array(&f.e, [1_000 * UNIT, 1_000 * UNIT]);
    f.pool.add_liquidity(&f.user, &amounts, &0);

    let amount_in = 100 * UNIT;
    let quoted = f.pool.get_quote(&f.token_a, &f.token_b, &amount_in);
    let dy = f.pool.swap(&f.user, &f.token_a, &f.token_b, &amount_in, &0);

    // get_quote must match the actual swap output on unchanged state.
    assert_eq!(quoted, dy);

    // Stable-swap on a near-balanced pool: far less slippage than a
    // constant-product AMM would produce (which would give ~90.9 here).
    assert!(dy > 99 * UNIT, "dy={dy} too low for a stable pool");
    assert!(dy < amount_in, "fee/slippage should make dy < amount_in");

    let reserves = f.pool.get_reserves();
    assert_eq!(reserves.get(0).unwrap(), 1_100 * UNIT);
    assert_eq!(reserves.get(1).unwrap(), 1_000 * UNIT - dy);
}

#[test]
fn remove_liquidity_requires_prior_lp_approval_and_returns_pro_rata_share() {
    let f = setup();
    let amounts = Vec::from_array(&f.e, [1_000 * UNIT, 1_000 * UNIT]);
    let minted = f.pool.add_liquidity(&f.user, &amounts, &0);

    let pool_address = f.pool.address.clone();
    f.lp.approve(&f.user, &pool_address, &minted, &(f.e.ledger().sequence() + 100));

    let withdraw = minted / 2;
    let min_amounts = Vec::from_array(&f.e, [0i128, 0i128]);
    let out = f.pool.remove_liquidity(&f.user, &withdraw, &min_amounts);

    assert_eq!(out.get(0).unwrap(), 500 * UNIT);
    assert_eq!(out.get(1).unwrap(), 500 * UNIT);
    assert_eq!(f.lp.total_supply(), minted - withdraw);
    assert_eq!(f.lp.balance(&f.user), minted - withdraw);
}

#[test]
fn ramp_amp_interpolates_linearly_over_time() {
    let f = setup();
    let amounts = Vec::from_array(&f.e, [1_000 * UNIT, 1_000 * UNIT]);
    f.pool.add_liquidity(&f.user, &amounts, &0);

    let start = f.e.ledger().timestamp();
    let ramp_end = start + 1_000;
    f.pool.ramp_amp(&(AMP * 2), &ramp_end);

    // Halfway through the ramp, amp should be roughly the midpoint.
    f.e.ledger().set_timestamp(start + 500);
    let mid_amp = f.pool.get_amp();
    assert!(
        mid_amp > AMP && mid_amp < AMP * 2,
        "amp should be strictly between start and target mid-ramp, got {mid_amp}"
    );

    f.e.ledger().set_timestamp(ramp_end);
    assert_eq!(f.pool.get_amp(), AMP * 2);
}

#[test]
fn swap_rejects_same_token() {
    let f = setup();
    let amounts = Vec::from_array(&f.e, [100 * UNIT, 100 * UNIT]);
    f.pool.add_liquidity(&f.user, &amounts, &0);

    let result = f
        .pool
        .try_swap(&f.user, &f.token_a, &f.token_a, &UNIT, &0);
    assert!(result.is_err());
}

#[test]
fn swap_rejects_slippage_below_min_out() {
    let f = setup();
    let amounts = Vec::from_array(&f.e, [1_000 * UNIT, 1_000 * UNIT]);
    f.pool.add_liquidity(&f.user, &amounts, &0);

    let result = f
        .pool
        .try_swap(&f.user, &f.token_a, &f.token_b, &(100 * UNIT), &(1_000 * UNIT));
    assert!(result.is_err());
}
