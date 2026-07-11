//! Requires `pool` and `lp-token` to already be built for
//! `wasm32v1-none` release (see `contracts/scripts/deploy.sh` step 1, or
//! run `cargo build -p pool -p lp-token --target wasm32v1-none --release`
//! from `contracts/` before `cargo test -p factory`), since these tests
//! exercise the real cross-contract deploy path rather than a mock.

use crate::{Factory, FactoryClient};
use soroban_sdk::{testutils::Address as _, token, Address, Env, String, Vec};

mod pool_wasm {
    soroban_sdk::contractimport!(
        file = "../pool/target/wasm32v1-none/release/pool.wasm"
    );
}
mod lp_token_wasm {
    soroban_sdk::contractimport!(
        file = "../lp-token/target/wasm32v1-none/release/lp_token.wasm"
    );
}

const UNIT: i128 = 10_000_000;

struct Fixture<'a> {
    e: Env,
    factory: FactoryClient<'a>,
    admin: Address,
    usdc: Address,
    ngnc: Address,
    user: Address,
}

fn setup() -> Fixture<'static> {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let user = Address::generate(&e);

    let usdc = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let ngnc = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    for t in [&usdc, &ngnc] {
        token::StellarAssetClient::new(&e, t).mint(&user, &(1_000_000 * UNIT));
    }

    let pool_wasm_hash = e.deployer().upload_contract_wasm(pool_wasm::WASM);
    let lp_wasm_hash = e.deployer().upload_contract_wasm(lp_token_wasm::WASM);

    let factory_id = e.register_contract(None, Factory);
    let factory = FactoryClient::new(&e, &factory_id);
    factory.initialize(&admin, &pool_wasm_hash, &lp_wasm_hash);

    Fixture {
        e,
        factory,
        admin,
        usdc,
        ngnc,
        user,
    }
}

#[test]
fn deploy_pool_wires_pool_and_lp_token_together() {
    let f = setup();
    let tokens = Vec::from_array(&f.e, [f.usdc.clone(), f.ngnc.clone()]);

    let pool_address = f.factory.deploy_pool(
        &tokens,
        &100u128,
        &30u32,
        &String::from_str(&f.e, "NairaBridge USDC-NGNC LP"),
        &String::from_str(&f.e, "nbLP"),
    );

    assert_eq!(
        f.factory.get_pool(&f.usdc, &f.ngnc),
        Some(pool_address.clone())
    );
    assert_eq!(f.factory.all_pools(), Vec::from_array(&f.e, [pool_address.clone()]));

    // The deployed pool should be immediately usable end-to-end: add
    // liquidity through it and confirm reserves update.
    let pool_client = pool_wasm::Client::new(&f.e, &pool_address);
    let minted = pool_client.add_liquidity(
        &f.user,
        &Vec::from_array(&f.e, [100 * UNIT, 100 * UNIT]),
        &0,
    );
    assert_eq!(minted, 200 * UNIT);
}

#[test]
fn deploy_pool_rejects_duplicate_pair() {
    let f = setup();
    let tokens = Vec::from_array(&f.e, [f.usdc.clone(), f.ngnc.clone()]);
    f.factory.deploy_pool(
        &tokens,
        &100u128,
        &30u32,
        &String::from_str(&f.e, "LP"),
        &String::from_str(&f.e, "LP"),
    );

    let result = f.factory.try_deploy_pool(
        &tokens,
        &100u128,
        &30u32,
        &String::from_str(&f.e, "LP2"),
        &String::from_str(&f.e, "LP2"),
    );
    assert!(result.is_err());
}

#[test]
fn ramp_pool_amp_is_forwarded_through_factory_admin() {
    let f = setup();
    let tokens = Vec::from_array(&f.e, [f.usdc.clone(), f.ngnc.clone()]);
    let pool_address = f.factory.deploy_pool(
        &tokens,
        &100u128,
        &30u32,
        &String::from_str(&f.e, "LP"),
        &String::from_str(&f.e, "LP"),
    );

    let pool_client = pool_wasm::Client::new(&f.e, &pool_address);
    let ramp_end = f.e.ledger().timestamp() + 1_000;
    f.factory.ramp_pool_amp(&pool_address, &200u128, &ramp_end);

    let _ = f.admin;
    assert_eq!(pool_client.get_amp(), 100u128); // ramp hasn't started elapsing yet
}
