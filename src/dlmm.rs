//! DLMM fee schedule and launch-phase surcharge policies

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// DLMM fee schedule in *decimal* space for simulation.
/// 
/// Implements base+variable fee model where variable fee grows with (volatility_accumulator * bin_step)^2.
/// The dev formulas page shows OFFSET/SCALE for integer precision; we keep decimal here.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DlmmFeeParams {
    /// Base factor B (dimensionless)
    pub base_factor: f64,
    /// Bin step in *bps* (e.g., 10 for 0.10%). Converted to decimal internally.
    pub bin_step_bps: f64,
    /// Variable fee control A (dimensionless)
    pub variable_fee_control: f64,
    /// Max total fee (decimal, e.g., 0.05 = 5%). If you pass ~1e8, we treat it as integer cap.
    pub max_fee_rate: f64,

    /// Fee offset for future integer-mode emulation (currently not used in decimal mode)
    pub fee_offset: f64,
    /// Fee scale for future integer-mode emulation (currently not used in decimal mode)
    pub fee_scale: f64,
}

impl DlmmFeeParams {
    #[inline] fn s_dec(&self) -> f64 { self.bin_step_bps / 10_000.0 }

    /// Base fee f_b = B * s (decimal).
    pub fn base_fee_rate(&self) -> f64 { self.base_factor * self.s_dec() }

    /// Variable fee f_v = A * (va * s)^2 (decimal).
    pub fn variable_fee_rate(&self, volatility_accumulator: f64) -> f64 {
        let s = self.s_dec();
        self.variable_fee_control * (volatility_accumulator * s).powi(2)
    }

    /// Total fee (decimal), capped. Integer-looking caps auto-converted (cap/1e8).
    pub fn total_fee_rate(&self, va: f64) -> f64 {
        let cap = if self.max_fee_rate > 1.0 { self.max_fee_rate / 1e8 } else { self.max_fee_rate }.max(0.0);
        (self.base_fee_rate() + self.variable_fee_rate(va)).min(cap)
    }

    /// Price impact guards (per docs).
    /// Selling X for Y: min_price = spot * 10000 / (10000 - impact_bps)
    pub fn min_price_sell_x_for_y(spot_price: f64, max_price_impact_bps: f64) -> f64 {
        spot_price * 10_000.0 / (10_000.0 - max_price_impact_bps)
    }
    /// Selling Y for X: min_price = spot * (10000 - impact_bps) / 10000
    pub fn min_price_sell_y_for_x(spot_price: f64, max_price_impact_bps: f64) -> f64 {
        spot_price * (10_000.0 - max_price_impact_bps) / 10_000.0
    }
}

/// A neutral, policy-level surcharge active during the launch phase.
/// 
/// The surcharge τ(t) decays linearly from τ0 to τ1 over [0, T].
#[derive(Default, Clone)]
pub struct LaunchPhaseSurcharge {
    /// Set of whitelisted addresses exempt from surcharge
    pub whitelist: HashSet<String>,
    /// Initial surcharge percentage at launch (t=0)
    pub tau_start_pct: f64,
    /// Final surcharge percentage after ramp period
    pub tau_end_pct: f64,
    /// Duration of the ramp period in seconds
    pub ramp_secs: f64,
}
impl LaunchPhaseSurcharge {
    /// Checks if an address is whitelisted
    pub fn is_whitelisted(&self, addr: &str) -> bool { self.whitelist.contains(addr) }
    /// Calculates the surcharge percentage at a given time since launch
    pub fn tau(&self, seconds_since_launch: f64) -> f64 {
        if seconds_since_launch <= 0.0 { return self.tau_start_pct.max(self.tau_end_pct); }
        if seconds_since_launch >= self.ramp_secs { return self.tau_end_pct; }
        let t = seconds_since_launch / self.ramp_secs;
        self.tau_start_pct + t * (self.tau_end_pct - self.tau_start_pct)
    }
}