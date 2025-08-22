//! DLMM fee schedule and launch-phase launch policy (allowlist + time-decay surcharge)

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// DLMM fee schedule in decimal space.
/// f = f_b + f_v, with f_b = B·s and f_v = A·(va·s)^2, capped at `max_fee_rate` (decimal, e.g. 0.05 = 5%).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DlmmFeeParams {
    /// Base factor B (dimensionless)
    pub base_factor: f64,
    /// Bin step in *bps* (e.g., 10 for 0.10%). Converted to decimal internally.
    pub bin_step_bps: f64,
    /// Variable fee control A (dimensionless)
    pub variable_fee_control: f64,
    /// Max total fee (decimal, e.g., 0.05 = 5%).
    pub max_fee_rate: f64,
}

impl DlmmFeeParams {
    #[inline]
    fn s_dec(&self) -> f64 {
        self.bin_step_bps / 10_000.0
    }

    /// Base fee f_b = B * s (decimal).
    pub fn base_fee_rate(&self) -> f64 {
        self.base_factor * self.s_dec()
    }

    /// Variable fee f_v = A * (va * s)^2 (decimal).
    pub fn variable_fee_rate(&self, volatility_accumulator: f64) -> f64 {
        let s = self.s_dec();
        self.variable_fee_control * (volatility_accumulator * s).powi(2)
    }

    /// Total fee (decimal), capped at `max_fee_rate` (must be ≤ 1.0).
    pub fn total_fee_rate(&self, va: f64) -> f64 {
        let cap = self.max_fee_rate.max(0.0);
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

/// Launch-phase policy: allowlist + time-decaying surcharge τ(t) from τ0 to τ1 over [0, T].
#[derive(Default, Clone, Debug)]
pub struct LaunchPhasePolicy {
    /// Addresses exempt from the surcharge
    pub allowlist: HashSet<String>,
    /// Initial surcharge percentage at launch (t=0)
    pub tau_start_pct: f64,
    /// Final surcharge percentage after ramp period
    pub tau_end_pct: f64,
    /// Duration of the ramp period in seconds
    pub ramp_secs: f64,
}
impl LaunchPhasePolicy {
    /// Checks if an address is exempt from launch phase surcharges.
    /// 
    /// This is a core API method for integrators implementing launch phase policies.
    /// Addresses on the allowlist can trade without paying the time-decaying surcharge.
    /// 
    /// # Example
    /// ```
    /// use bcurve::dlmm::LaunchPhasePolicy;
    /// use std::collections::HashSet;
    /// 
    /// let mut allowlist = HashSet::new();
    /// allowlist.insert("whitelisted_user_123".to_string());
    /// 
    /// let policy = LaunchPhasePolicy {
    ///     allowlist,
    ///     tau_start_pct: 50.0,
    ///     tau_end_pct: 3.0,
    ///     ramp_secs: 60.0,
    /// };
    /// 
    /// assert!(policy.is_allowed("whitelisted_user_123"));
    /// assert!(!policy.is_allowed("regular_user_456"));
    /// ```
    #[allow(dead_code)] // Public API for library integrators, not used by CLI
    pub fn is_allowed(&self, addr: &str) -> bool {
        self.allowlist.contains(addr)
    }
    /// Calculates the surcharge percentage at a given time since launch
    pub fn tau(&self, seconds_since_launch: f64) -> f64 {
        if seconds_since_launch <= 0.0 {
            return self.tau_start_pct.max(self.tau_end_pct);
        }
        if seconds_since_launch >= self.ramp_secs {
            return self.tau_end_pct;
        }
        let t = seconds_since_launch / self.ramp_secs;
        self.tau_start_pct + t * (self.tau_end_pct - self.tau_start_pct)
    }
}
