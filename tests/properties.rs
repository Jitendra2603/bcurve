use approx::assert_relative_eq;
use bcurve::curves::{Curve, Geometric, Grid, LogisticS};
use bcurve::dlmm::DlmmFeeParams;
use proptest::prelude::*;

proptest! {
    #[test]
    fn geometric_closed_form_matches_sum(
        p0 in 1e-6f64..1e1,  // avoid degenerate zeros
        step_bps in 1.0f64..100.0, // 0.01%..1%
        theta in 0.1f64..0.99,
        r0 in 1e-6f64..1e6,
        n in 1i64..2000
    ) {
        let grid = Grid { p0, bin_step_bps: step_bps };
        let g = Geometric { grid, theta, r0_quote: r0 };
        let mut s_num = 0.0;
        for i in 0..n { s_num += g.delta_x_of_bin(i); }
        let s_cf = g.s_n_closed(n);
        assert_relative_eq!(s_num, s_cf, max_relative = 1e-9);
        let mut prev = f64::NEG_INFINITY;
        for i in 0..n {
            let p = g.price_of_bin(i);
            assert!(p > prev);
            prev = p;
        }
    }

    #[test]
    fn fees_are_monotone_in_va(
        step_bps in 1.0f64..100.0,
        base in 0.0f64..1.0,
        varc in 0.0f64..1.0,
        cap in 0.001f64..0.50,  // 0.1%..50% cap
        va1 in 0.0f64..50.0,
        va2 in 0.0f64..50.0,
    ) {
        let f = DlmmFeeParams {
            base_factor: base,
            bin_step_bps: step_bps,
            variable_fee_control: varc,
            max_fee_rate: cap,
        };
        let t1 = f.total_fee_rate(va1);
        let t2 = f.total_fee_rate(va2);
        // Not strictly monotone if the cap is hit; but in the unconstrained region
        // increasing VA should not *decrease* fees.
        if t1 < cap && t2 < cap {
            let v1 = f.variable_fee_rate(va1);
            let v2 = f.variable_fee_rate(va2);
            // Quadratic in VA: larger VA => larger var fee
            prop_assert!((va1 - va2).abs() < 1e-12 || (v1 - v2).signum() == (va1 - va2).signum());
        }
        // total fee never exceeds cap
        prop_assert!(t1 <= cap + 1e-15);
        prop_assert!(t2 <= cap + 1e-15);
    }

    #[test]
    fn logistic_allocations_are_nonnegative(
        p0 in 1e-6f64..1e1,
        step_bps in 1.0f64..100.0,
        pmin in 1e-8f64..1e-3,
        pmax in 1e-2f64..1e1,
        k in 1e-10f64..1e-5,
        n in 2i64..2000
    ) {
        prop_assume!(pmin < p0 && p0 < pmax);
        let grid = Grid { p0, bin_step_bps: step_bps };
        // choose s_mid so S(P0)=0
        let s_mid = ((pmax - p0)/(p0 - pmin)).ln() / k;
        let cur = LogisticS { grid, p_min: pmin, p_max: pmax, k, s_mid, bins: n };
        for i in 0..n-1 {
            let dx = cur.delta_x_of_bin(i);
            prop_assert!(dx >= 0.0, "ΔX_i must be ≥ 0; got {dx} at i={i}");
        }
    }

    #[test]
    fn bin_inversion_respects_end_price(
        p0 in 1e-6f64..1e1,
        step_bps in 1.0f64..100.0, // 0.01%..1%
        ratio in 1.001f64..10.0
    ) {
        let q = 1.0 + step_bps / 10_000.0;
        let p_end = p0 * ratio;
        // n = ceil( ln(end/p0) / ln(q) )
        let n = ( (p_end/p0).ln() / q.ln() ).ceil() as i64;
        let p_n = p0 * q.powi(n as i32);
        prop_assert!(p_n + 1e-15 >= p_end, "p_n={} < p_end={}", p_n, p_end);
    }
}
