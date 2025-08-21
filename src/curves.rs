//! Bonding curve implementations for DLMM

use serde::{Deserialize, Serialize};

/// Generic interface for bonding curves on a DLMM price grid
pub trait Curve {
    /// Returns the name/type of this curve implementation
    fn name(&self) -> &'static str;
    
    /// Returns the price at bin index i: P_i = P_0 * q^i
    fn price_of_bin(&self, i: i64) -> f64;
    
    /// Returns the token allocation for bin i
    fn delta_x_of_bin(&self, i: i64) -> f64;

    /// Computes the cumulative supply from bin 0 to n-1
    fn cumulative_supply(&self, n: i64) -> f64 {
        let mut s = 0.0;
        for i in 0..n { s += self.delta_x_of_bin(i); }
        s
    }
}

/// DLMM price grid parameters
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Grid {
    /// Initial price at bin 0
    pub p0: f64,
    /// Bin step size in basis points (e.g., 10 = 0.10%)
    pub bin_step_bps: f64,
}
impl Grid {
    /// Returns the growth factor q = 1 + bin_step_bps/10,000
    pub fn q(&self) -> f64 { 1.0 + self.bin_step_bps / 10_000.0 }
    /// Returns the price at bin i: P_i = P_0 * q^i
    pub fn price_of_bin(&self, i: i64) -> f64 { self.p0 * self.q().powi(i as i32) }
}

/// Geometric bonding curve: ΔX_i = (R_0/P_0) * r^i where r = q^(θ-1)
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Geometric {
    /// DLMM price grid configuration
    pub grid: Grid,
    /// Steepness parameter θ (typically 0 < θ < 1)
    pub theta: f64,
    /// Initial quote revenue R_0 in bin 0
    pub r0_quote: f64,
}
impl Geometric {
    /// Returns the decay factor r = q^(θ-1)
    pub fn r(&self) -> f64 { self.grid.q().powf(self.theta - 1.0) }
    /// Returns the growth factor g = q^θ
    pub fn g(&self) -> f64 { self.grid.q().powf(self.theta) }
    /// Returns the initial token allocation ΔX_0 = R_0/P_0
    pub fn delta_x0(&self) -> f64 { self.r0_quote / self.grid.p0 }
    /// Computes the closed-form cumulative supply S_n
    pub fn s_n_closed(&self, n: i64) -> f64 {
        let r = self.r();
        if (r - 1.0).abs() < 1e-12 { self.delta_x0() * n as f64 }
        else { self.delta_x0() * (1.0 - r.powi(n as i32)) / (1.0 - r) }
    }
    /// Solves for R_0 given a target total supply S_n
    pub fn solve_r0_from_supply(&self, target_s: f64, n: i64) -> f64 {
        let r = self.r();
        if (r - 1.0).abs() < 1e-12 {
            // In the r→1 limit, S_n = ΔX_0 * n  ⇒  ΔX_0 = target_s / n
            (target_s / (n as f64)) * self.grid.p0
        } else {
            let denom = 1.0 - r.powi(n as i32);
            let a = target_s * (1.0 - r) / denom;
            a * self.grid.p0
        }
    }
}
impl Curve for Geometric {
    fn name(&self) -> &'static str { "DLMM-Geometric(θ)" }
    fn price_of_bin(&self, i: i64) -> f64 { self.grid.price_of_bin(i) }
    fn delta_x_of_bin(&self, i: i64) -> f64 { self.delta_x0() * self.r().powi(i as i32) }
}

/// Logistic target P(S) discretized onto the DLMM grid via ΔX_i = S(P_{i+1}) - S(P_i)
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct LogisticS {
    /// DLMM price grid configuration
    pub grid: Grid,
    /// Minimum price asymptote
    pub p_min: f64,
    /// Maximum price asymptote
    pub p_max: f64,
    /// Steepness parameter (larger k = steeper curve)
    pub k: f64,
    /// Midpoint supply where P(s_mid) = (p_min + p_max)/2
    pub s_mid: f64,
    /// Total number of bins
    pub bins: i64,
}
impl LogisticS {
    fn s_of_p(&self, p: f64) -> f64 {
        let eps = (self.p_max - self.p_min) * 1e-12;
        let p = p.clamp(self.p_min + eps, self.p_max - eps);
        let num = self.p_max - p;
        let den = p - self.p_min;
        self.s_mid - (num / den).ln() / self.k
    }
    fn s_i(&self, i: i64) -> f64 { self.s_of_p(self.grid.price_of_bin(i)) }
}
impl Curve for LogisticS {
    fn name(&self) -> &'static str { "Logistic-S(on DLMM bins)" }
    fn price_of_bin(&self, i: i64) -> f64 { self.grid.price_of_bin(i) }
    fn delta_x_of_bin(&self, i: i64) -> f64 {
        if i + 1 >= self.bins { return 0.0; }
        let s_i = self.s_i(i);
        let s_ip1 = self.s_i(i + 1);
        (s_ip1 - s_i).max(0.0)
    }
}