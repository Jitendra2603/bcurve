//! Verification tools for curve properties and numerical accuracy

use crate::curves::{Curve, Geometric};
use anyhow::{anyhow, Result};

/// Verification report containing numerical checks and validation results
#[derive(Debug)]
pub struct Report {
    /// Number of bins checked
    pub bins: i64,
    /// Numerical sum of supply
    pub supply_sum: f64,
    /// Closed-form supply calculation (if available)
    pub supply_closed: Option<f64>,
    /// Relative error between sum and closed form
    pub rel_err_supply: Option<f64>,
    /// Whether price monotonicity holds
    pub monotone_ok: bool,
}

/// Verify S_n = Σ_{i<n} ΔX_0 r^i against the closed form and check P_i monotonicity
pub fn verify_geometric(c: &Geometric, bins: i64) -> Result<Report> {
    let mut s_sum = 0.0_f64;
    let mut comp = 0.0_f64;

    let mut prev_px = f64::NEG_INFINITY;
    let mut monotone_ok = true;

    for i in 0..bins {
        let dx = c.delta_x_of_bin(i);
        if dx < 0.0 { return Err(anyhow!("ΔX_{} < 0", i)); }
        let t = s_sum + dx;
        if s_sum.abs() >= dx.abs() { comp += (s_sum - t) + dx; } else { comp += (dx - t) + s_sum; }
        s_sum = t;

        let p = c.price_of_bin(i);
        if p <= prev_px { monotone_ok = false; }
        prev_px = p;
    }
    let s_sum = s_sum + comp;

    let s_closed = c.s_n_closed(bins);
    let rel = if s_closed.abs() > 0.0 { (s_sum - s_closed).abs() / s_closed.abs() } else { 0.0 };

    Ok(Report {
        bins,
        supply_sum: s_sum,
        supply_closed: Some(s_closed),
        rel_err_supply: Some(rel),
        monotone_ok,
    })
}