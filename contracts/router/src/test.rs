use crate::{Router, RouterClient};
use lp_token::{LpToken, LpTokenClient};
use pool::{Pool, PoolClient};
use soroban_sdk::{testutils::Address as _, token, Address, Env, String, Vec};

const UNIT: i128 = 10_000_000; // 7 decimals

struct Fixture<'a> {
    e: Env,
    router: RouterClient<'a>,
    usdc: Address,
    xlm: Address,
    ngnc: Address,
    user: Address,
}

fn deploy_pool<'a>(
    e: &Env,
    admin: &Address,
    token_a: &Address,
    token_b: &Address,
) -> (PoolClient<'a>, Address) {
    let pool_id = e.register_contract(None, Pool);
    let pool = PoolClient::new(e, &pool_id);
    let tokens = Vec::from_array(e, [token_a.clone(), token_b.clone()]);
    pool.initialize(&tokens, &100u128, &30u32, admin);

    let lp_id = e.register_contract(None, LpToken);
    let lp = LpTokenClient::new(e, &lp_id);
    lp.initialize(
        &pool_id,
        &7,
        &String::from_str(e, "LP"),
        &String::from_str(e, "LP"),
    );
    pool.set_lp_token(&lp_id);

    (pool, pool_id)
}

fn setup() -> Fixture<'static> {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let user = Address::generate(&e);

    let usdc = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let xlm = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let ngnc = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    for t in [&usdc, &xlm, &ngnc] {
        token::StellarAssetClient::new(&e, t).mint(&user, &(1_000_000 * UNIT));
    }

    let router_id = e.register_contract(None, Router);
    let router = RouterClient::new(&e, &router_id);
    router.initialize(&admin);

    // Direct USDC/NGNC pool (thin, so hub routing via XLM can beat it).
    let (usdc_ngnc_pool, usdc_ngnc_id) = deploy_pool(&e, &admin, &usdc, &ngnc);
    usdc_ngnc_pool.add_liquidity(
        &user,
        &Vec::from_array(&e, [10 * UNIT, 10 * UNIT]),
        &0,
    );
    router.register_pool(&usdc, &ngnc, &usdc_ngnc_id);

    // Deeper USDC/XLM and XLM/NGNC pools forming the hub route.
    let (usdc_xlm_pool, usdc_xlm_id) = deploy_pool(&e, &admin, &usdc, &xlm);
    usdc_xlm_pool.add_liquidity(
        &user,
        &Vec::from_array(&e, [10_000 * UNIT, 10_000 * UNIT]),
        &0,
    );
    router.register_pool(&usdc, &xlm, &usdc_xlm_id);

    let (xlm_ngnc_pool, xlm_ngnc_id) = deploy_pool(&e, &admin, &xlm, &ngnc);
    xlm_ngnc_pool.add_liquidity(
        &user,
        &Vec::from_array(&e, [10_000 * UNIT, 10_000 * UNIT]),
        &0,
    );
    router.register_pool(&xlm, &ngnc, &xlm_ngnc_id);

    Fixture {
        e,
        router,
        usdc,
        xlm,
        ngnc,
        user,
    }
}

#[test]
fn find_best_route_prefers_deeper_hub_route_over_thin_direct_pool() {
    let f = setup();
    let amount_in = 5 * UNIT;

    let (path, pools, out) = f
        .router
        .find_best_route(&f.usdc, &f.ngnc, &amount_in);

    assert_eq!(path.len(), 3);
    assert_eq!(path.get(0).unwrap(), f.usdc);
    assert_eq!(path.get(1).unwrap(), f.xlm);
    assert_eq!(path.get(2).unwrap(), f.ngnc);
    assert_eq!(pools.len(), 2);
    assert!(out > 0);
}

#[test]
fn swap_exact_in_executes_multi_hop_route_and_delivers_funds_to_user() {
    let f = setup();
    let amount_in = 5 * UNIT;

    let (path, pools, quoted_out) = f
        .router
        .find_best_route(&f.usdc, &f.ngnc, &amount_in);

    let balance_before = token::Client::new(&f.e, &f.ngnc).balance(&f.user);
    let out = f
        .router
        .swap_exact_in(&f.user, &path, &pools, &amount_in, &0);
    let balance_after = token::Client::new(&f.e, &f.ngnc).balance(&f.user);

    assert_eq!(out, quoted_out);
    assert_eq!(balance_after - balance_before, out);
}

#[test]
fn swap_exact_in_rejects_slippage_below_min_out() {
    let f = setup();
    let amount_in = 5 * UNIT;
    let (path, pools, _) = f.router.find_best_route(&f.usdc, &f.ngnc, &amount_in);

    let result = f
        .router
        .try_swap_exact_in(&f.user, &path, &pools, &amount_in, &(1_000_000 * UNIT));
    assert!(result.is_err());
}

#[test]
fn find_best_route_returns_empty_when_no_route_exists() {
    let f = setup();
    let stray = Address::generate(&f.e);
    let (path, pools, out) = f.router.find_best_route(&f.usdc, &stray, &UNIT);
    assert_eq!(path.len(), 0);
    assert_eq!(pools.len(), 0);
    assert_eq!(out, 0);
}
