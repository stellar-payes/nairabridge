//! StableSwap (Curve-style) invariant math, in fixed-point i128, for pools of
//! like-valued assets (e.g. USDC/NGNC/EURC).
//!
//! `no_std` and allocation-free: reserves are passed as slices bounded by
//! [`MAX_COINS`] and copied into stack arrays internally, so this crate can be
//! linked directly into a Soroban contract or published standalone to
//! crates.io for reuse by other Soroban stable-pool projects.
//!
//! Invariant (n coins, amplification A):
//! `A * n^n * sum(x) + D = A * D * n^n + D^(n+1) / (n^n * prod(x))`
//!
//! Both solvers use Newton's method, matching the reference implementation
//! used by Curve Finance's StableSwap pools.

#![cfg_attr(not(test), no_std)]

/// Maximum number of coins a pool built on this crate can hold (supports up
/// to a tri-pool, e.g. USDC/NGNC/EURC).
pub const MAX_COINS: usize = 3;

/// Newton's method iteration cap. Both solvers converge in ~10 iterations
/// for any realistic pool state; this is a generous upper bound to guard
/// against pathological inputs rather than a value we expect to hit.
const MAX_ITERATIONS: u16 = 255;

/// Compute the StableSwap invariant `D` for the given reserves.
///
/// `xp` are reserves normalized to the same number of decimals (the caller
/// is responsible for scaling assets with different decimals before calling
/// in). `amp` is the amplification coefficient `A` (not pre-multiplied by
/// `n`).
///
/// Returns `None` on overflow, invalid input (empty or oversized reserve
/// list, zero balances), or failure to converge.
pub fn get_d(xp: &[i128], amp: u128) -> Option<i128> {
    let n_coins = xp.len();
    if n_coins == 0 || n_coins > MAX_COINS || xp.iter().any(|&x| x <= 0) {
        return None;
    }
    let n = n_coins as i128;

    let sum: i128 = xp.iter().try_fold(0i128, |acc, &x| acc.checked_add(x))?;
    let ann = (amp as i128).checked_mul(n)?;
    let mut d = sum;

    for _ in 0..MAX_ITERATIONS {
        // d_p = D^(n+1) / (n^n * prod(x))
        let mut d_p = d;
        for &x in xp {
            d_p = d_p.checked_mul(d)?.checked_div(x.checked_mul(n)?)?;
        }
        let d_prev = d;

        // D = (Ann * sum + D_P * n) * D / ((Ann - 1) * D + (n + 1) * D_P)
        let numerator = ann
            .checked_mul(sum)?
            .checked_add(d_p.checked_mul(n)?)?
            .checked_mul(d)?;
        let denominator = ann
            .checked_sub(1)?
            .checked_mul(d)?
            .checked_add(d_p.checked_mul(n.checked_add(1)?)?)?;
        if denominator == 0 {
            return None;
        }
        d = numerator.checked_div(denominator)?;

        let diff = if d > d_prev { d - d_prev } else { d_prev - d };
        if diff <= 1 {
            return Some(d);
        }
    }
    None
}

/// Solve for the balance of coin `token_index` that satisfies invariant `d`,
/// given the (already updated) balances of every other coin in `xp`.
///
/// `xp[token_index]` is ignored on input — only the other entries are used —
/// and the returned value is the new balance for that index.
pub fn get_y(xp: &[i128], amp: u128, token_index: usize, d: i128) -> Option<i128> {
    let n_coins = xp.len();
    if n_coins == 0
        || n_coins > MAX_COINS
        || token_index >= n_coins
        || d <= 0
        || xp.iter().enumerate().any(|(i, &x)| i != token_index && x <= 0)
    {
        return None;
    }
    let n = n_coins as i128;
    let ann = (amp as i128).checked_mul(n)?;

    let mut c = d;
    let mut sum = 0i128;
    for (i, &x) in xp.iter().enumerate() {
        if i == token_index {
            continue;
        }
        sum = sum.checked_add(x)?;
        c = c.checked_mul(d)?.checked_div(x.checked_mul(n)?)?;
    }
    c = c.checked_mul(d)?.checked_div(ann.checked_mul(n)?)?;
    let b = sum.checked_add(d.checked_div(ann)?)?;

    let mut y = d;
    for _ in 0..MAX_ITERATIONS {
        let y_prev = y;
        // y = (y^2 + c) / (2y + b - D)
        let numerator = y.checked_mul(y)?.checked_add(c)?;
        let denominator = y.checked_mul(2)?.checked_add(b)?.checked_sub(d)?;
        if denominator == 0 {
            return None;
        }
        y = numerator.checked_div(denominator)?;

        let diff = if y > y_prev { y - y_prev } else { y_prev - y };
        if diff <= 1 {
            return Some(y);
        }
    }
    None
}

/// Compute the raw (pre-fee) output amount for swapping `dx` of coin `i`
/// into coin `j`, given current reserves `xp`.
pub fn swap_to(xp: &[i128], amp: u128, i: usize, j: usize, dx: i128) -> Option<i128> {
    let n_coins = xp.len();
    if i == j || i >= n_coins || j >= n_coins || dx <= 0 || n_coins > MAX_COINS {
        return None;
    }

    let d0 = get_d(xp, amp)?;

    let mut xp_new = [0i128; MAX_COINS];
    xp_new[..n_coins].copy_from_slice(xp);
    xp_new[i] = xp[i].checked_add(dx)?;

    let y = get_y(&xp_new[..n_coins], amp, j, d0)?;
    // Round in the pool's favor: the trader receives one unit less than the
    // exact solution would allow.
    let dy = xp[j].checked_sub(y)?.checked_sub(1)?;
    if dy <= 0 {
        return None;
    }
    Some(dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Floating-point re-implementation of the same Newton's-method solvers,
    /// used purely as an independent cross-check on the fixed-point integer
    /// math in tests below (never used at contract runtime).
    mod reference {
        pub fn get_d(xp: &[f64], amp: f64) -> f64 {
            let n = xp.len() as f64;
            let sum: f64 = xp.iter().sum();
            if sum == 0.0 {
                return 0.0;
            }
            let ann = amp * n;
            let mut d = sum;
            for _ in 0..255 {
                let mut d_p = d;
                for &x in xp {
                    d_p = d_p * d / (x * n);
                }
                let d_prev = d;
                d = (ann * sum + d_p * n) * d / ((ann - 1.0) * d + (n + 1.0) * d_p);
                if (d - d_prev).abs() < 1e-6 {
                    break;
                }
            }
            d
        }

        pub fn get_y(xp: &[f64], amp: f64, token_index: usize, d: f64) -> f64 {
            let n = xp.len() as f64;
            let ann = amp * n;
            let mut c = d;
            let mut sum = 0.0;
            for (i, &x) in xp.iter().enumerate() {
                if i == token_index {
                    continue;
                }
                sum += x;
                c = c * d / (x * n);
            }
            c = c * d / (ann * n);
            let b = sum + d / ann;
            let mut y = d;
            for _ in 0..255 {
                let y_prev = y;
                y = (y * y + c) / (2.0 * y + b - d);
                if (y - y_prev).abs() < 1e-6 {
                    break;
                }
            }
            y
        }
    }

    const USDC_DECIMALS: i128 = 10_000_000; // 7 decimals, matches Soroban SEP-41 default

    #[test]
    fn balanced_two_coin_pool_d_equals_sum() {
        let xp = [1_000 * USDC_DECIMALS, 1_000 * USDC_DECIMALS];
        let d = get_d(&xp, 100).unwrap();
        // For a perfectly balanced pool, D == sum(x) exactly.
        assert_eq!(d, 2_000 * USDC_DECIMALS);
    }

    #[test]
    fn balanced_three_coin_pool_d_equals_sum() {
        let xp = [500 * USDC_DECIMALS, 500 * USDC_DECIMALS, 500 * USDC_DECIMALS];
        let d = get_d(&xp, 200).unwrap();
        assert_eq!(d, 1_500 * USDC_DECIMALS);
    }

    #[test]
    fn skewed_pool_matches_floating_point_reference() {
        let xp = [1_200 * USDC_DECIMALS, 800 * USDC_DECIMALS];
        let amp = 85u128;

        let d_int = get_d(&xp, amp).unwrap();
        let d_ref = reference::get_d(
            &[xp[0] as f64, xp[1] as f64],
            amp as f64,
        );

        let rel_err = ((d_int as f64) - d_ref).abs() / d_ref;
        assert!(rel_err < 1e-9, "D mismatch: int={d_int} ref={d_ref}");
    }

    #[test]
    fn get_y_matches_floating_point_reference() {
        let xp = [1_200 * USDC_DECIMALS, 800 * USDC_DECIMALS];
        let amp = 85u128;
        let d = get_d(&xp, amp).unwrap();

        let y_int = get_y(&xp, amp, 1, d).unwrap();
        let y_ref = reference::get_y(&[xp[0] as f64, xp[1] as f64], amp as f64, 1, d as f64);

        let rel_err = ((y_int as f64) - y_ref).abs() / y_ref;
        assert!(rel_err < 1e-9, "y mismatch: int={y_int} ref={y_ref}");
    }

    #[test]
    fn get_y_is_inverse_of_get_d() {
        // If we hold xp[0] fixed and solve for xp[1] given the pool's own D,
        // we should recover (approximately) the original xp[1].
        let xp = [1_100 * USDC_DECIMALS, 900 * USDC_DECIMALS];
        let d = get_d(&xp, 100).unwrap();
        let y = get_y(&xp, 100, 1, d).unwrap();
        assert!((y - xp[1]).abs() <= 1);
    }

    #[test]
    fn swap_moves_pool_toward_balance_and_conserves_d_within_rounding() {
        let xp = [1_000 * USDC_DECIMALS, 1_000 * USDC_DECIMALS];
        let amp = 100u128;
        let dx = 100 * USDC_DECIMALS;

        let d0 = get_d(&xp, amp).unwrap();
        let dy = swap_to(&xp, amp, 0, 1, dx).unwrap();

        // Stable-swap on a balanced pool near the peg: output is close to
        // input (near 1:1), unlike a constant-product curve.
        let slippage = (dx - dy) as f64 / dx as f64;
        assert!(slippage < 0.01, "unexpectedly high slippage: {slippage}");

        let xp_after = [xp[0] + dx, xp[1] - dy];
        let d1 = get_d(&xp_after, amp).unwrap();
        // D should not decrease (no fee applied here) and should not grow by
        // more than the rounding tolerance built into the solvers.
        assert!(d1 >= d0);
        assert!(d1 - d0 <= 2);
    }

    #[test]
    fn swap_rejects_invalid_indices() {
        let xp = [1_000 * USDC_DECIMALS, 1_000 * USDC_DECIMALS];
        assert!(swap_to(&xp, 100, 0, 0, 10).is_none());
        assert!(swap_to(&xp, 100, 0, 5, 10).is_none());
        assert!(swap_to(&xp, 100, 0, 1, 0).is_none());
    }

    #[test]
    fn get_d_rejects_too_many_coins() {
        let xp = [1i128; MAX_COINS + 1];
        assert!(get_d(&xp, 100).is_none());
    }

    proptest::proptest! {
        #[test]
        fn get_d_never_panics_on_arbitrary_balanced_reserves(
            a in 1_000_000i128..1_000_000_000_000i128,
            b in 1_000_000i128..1_000_000_000_000i128,
            amp in 1u128..5000u128,
        ) {
            // Should always either converge to a sane D or return None; must
            // never panic (overflow, div-by-zero) for any in-range input.
            let _ = get_d(&[a, b], amp);
        }

        #[test]
        fn swap_output_never_exceeds_input_reserve(
            a in 10_000_000i128..1_000_000_000_000i128,
            b in 10_000_000i128..1_000_000_000_000i128,
            dx in 1_000_000i128..100_000_000_000i128,
            amp in 1u128..5000u128,
        ) {
            if let Some(dy) = swap_to(&[a, b], amp, 0, 1, dx) {
                proptest::prop_assert!(dy < b);
            }
        }
    }
}
