mod curves;
mod dlmm;
mod plot;
mod verifier;

use crate::curves::{Curve, Geometric, Grid, LogisticS};
use crate::dlmm::{DlmmFeeParams, LaunchPhasePolicy};
use crate::plot::{plot_fee_vs_vol, plot_price_vs_supply, plot_tokens_per_bin};
use crate::verifier::verify_geometric;

use anyhow::{anyhow, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(
    name = "bcurve",
    version,
    about = "DLMM bonding curve simulator + verifier"
)]
struct Args {
    #[arg(long, default_value = "geometric")]
    mode: String,
    #[arg(long, default_value_t = 0.01)]
    p0: f64,
    #[arg(long, default_value_t = 10.0)]
    bin_step_bps: f64,

    /// θ (prefer 0<θ<1). θ>1 makes ΔX grow with i.
    #[arg(long, default_value_t = 0.6)]
    theta: f64,

    #[arg(long)]
    target_supply: Option<f64>,
    #[arg(long)]
    bins: Option<i64>,
    #[arg(long)]
    end_price: Option<f64>,
    #[arg(long)]
    r0: Option<f64>,

    #[arg(long, default_value_t = 0.0)]
    p_min: f64,
    #[arg(long)]
    p_max: Option<f64>,
    #[arg(long, default_value_t = 0.00001)]
    k: f64,
    #[arg(long, default_value_t = 0.0)]
    s_mid: f64,

    #[arg(long, default_value_t = 0.0)]
    base_factor: f64,
    #[arg(long, default_value_t = 0.0)]
    variable_fee_control: f64,
    #[arg(long, default_value_t = 0.0)]
    vol_accum: f64,
    #[arg(long, default_value_t = 0.10)]
    max_fee_rate: f64, // decimal default 10%

    // Launch-phase policy
    #[arg(long, default_value_t = 50.0)]
    tau_start_pct: f64,
    #[arg(long, default_value_t = 3.0)]
    tau_end_pct: f64,
    #[arg(long, default_value_t = 30.0)]
    tau_ramp_secs: f64,
    /// Path to a newline-separated allowlist; addresses here are exempt from τ(t)
    #[arg(long, alias = "whitelist-path")]
    allowlist_path: Option<String>,

    /// Optional: if provided, include price-guard metadata using this impact (bps)
    #[arg(long)]
    price_guard_bps: Option<f64>,

    #[arg(long, default_value = "out")]
    out_dir: String,
    #[arg(long = "no-draw", action = clap::ArgAction::SetFalse, default_value_t = true)]
    draw: bool,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Serialize, Deserialize)]
struct Row {
    bin: i64,
    price: f64,
    delta_x: f64,
    supply_cum: f64,
    revenue_bin: f64,
    revenue_cum: f64,
    fee_base: f64,
    fee_var: f64,
    fee_total: f64,
}

fn validate_inputs(args: &Args, grid: &Grid) -> Result<()> {
    if !grid.p0.is_finite() || grid.p0 <= 0.0 {
        return Err(anyhow!("p0 must be finite and > 0 (got {})", grid.p0));
    }
    if !grid.bin_step_bps.is_finite() || grid.bin_step_bps <= 0.0 {
        return Err(anyhow!(
            "bin_step_bps must be finite and > 0 (got {})",
            grid.bin_step_bps
        ));
    }
    if let Some(n) = args.bins {
        if n < 1 {
            return Err(anyhow!("bins must be ≥ 1 (got {})", n));
        }
    }
    if !(0.0..=1.0).contains(&args.max_fee_rate) {
        return Err(anyhow!(
            "max_fee_rate must be in [0,1] decimal (got {})",
            args.max_fee_rate
        ));
    }
    if let Some(bps) = args.price_guard_bps {
        if !(0.0..10_000.0).contains(&bps) {
            return Err(anyhow!(
                "price_guard_bps must be in [0, 10000) (got {})",
                bps
            ));
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let grid = Grid {
        p0: args.p0,
        bin_step_bps: args.bin_step_bps,
    };
    validate_inputs(&args, &grid)?;

    let mut allowlist = HashSet::new();
    if let Some(path) = &args.allowlist_path {
        if Path::new(path).exists() {
            for line in std::fs::read_to_string(path)?.lines() {
                let addr = line.trim();
                if !addr.is_empty() {
                    allowlist.insert(addr.to_string());
                }
            }
        }
    }
    let policy = LaunchPhasePolicy {
        allowlist,
        tau_start_pct: args.tau_start_pct,
        tau_end_pct: args.tau_end_pct,
        ramp_secs: args.tau_ramp_secs,
    };

    // fees
    let fees = DlmmFeeParams {
        base_factor: args.base_factor,
        bin_step_bps: args.bin_step_bps,
        variable_fee_control: args.variable_fee_control,
        max_fee_rate: args.max_fee_rate,
    };

    create_dir_all(&args.out_dir)?;

    match args.mode.as_str() {
        "geometric" => run_geometric(&args, grid, fees, policy),
        "logistic" => run_logistic(&args, grid, fees, policy),
        m => Err(anyhow!("unknown mode: {}", m)),
    }
}

fn compute_bins_from_end_price(grid: &Grid, end_price: f64) -> i64 {
    let q = grid.q();
    let ratio = end_price / grid.p0;
    assert!(ratio.is_finite(), "end_price / p0 must be finite");
    // Caller must ensure end_price > p0 for an increasing bin count.
    let n = (ratio.ln() / q.ln()).ceil() as i64;
    n.max(1)
}

fn run_geometric(
    args: &Args,
    grid: Grid,
    fees: DlmmFeeParams,
    policy: LaunchPhasePolicy,
) -> Result<()> {
    let bins = if let Some(n) = args.bins {
        n
    } else if let Some(p_end) = args.end_price {
        if p_end <= grid.p0 {
            return Err(anyhow!(
                "geometric: require end_price > p0; got end_price={} ≤ p0={}",
                p_end,
                grid.p0
            ));
        }
        compute_bins_from_end_price(&grid, p_end)
    } else {
        500
    };

    let theta = args.theta.clamp(-2.0, 2.0);
    let mut curve = Geometric {
        grid,
        theta,
        r0_quote: args.r0.unwrap_or(0.0),
    };

    if curve.r0_quote <= 0.0 {
        let target_s = args
            .target_supply
            .ok_or_else(|| anyhow!("geometric: need --r0 or --target-supply"))?;
        curve.r0_quote = curve.solve_r0_from_supply(target_s, bins);
    }

    let rep = verify_geometric(&curve, bins)?;
    if args.verbose {
        println!(
            "[{}] bins={} sumS={:.6} closed={:.6} rel_err={:.3e} monotone={}",
            curve.name(),
            rep.bins,
            rep.supply_sum,
            rep.supply_closed.unwrap(),
            rep.rel_err_supply.unwrap(),
            rep.monotone_ok
        );
        println!(
            "  Growth factor g=q^θ={:.12}, Decay factor r=q^(θ-1)={:.12}",
            curve.g(),
            curve.r()
        );
        println!(
            "  Cumulative supply at n={}: {:.6}",
            bins,
            curve.cumulative_supply(bins)
        );
        println!("  Allowlist size: {}", policy.allowlist.len());
        println!(
            "  Launch surcharge: τ(0s)={:.1}% → τ({:.0}s)={:.1}%",
            policy.tau(0.0),
            policy.ramp_secs,
            policy.tau(policy.ramp_secs)
        );
    }

    write_schedule_csv_geometric(
        &args.out_dir,
        &curve,
        bins,
        fees,
        args.vol_accum,
        &policy,
        args.price_guard_bps,
    )?;
    if args.draw {
        plot_price_vs_supply(
            &curve,
            bins,
            &format!("{}/price_vs_supply.png", &args.out_dir),
        )?;
        plot_tokens_per_bin(
            &curve,
            bins,
            &format!("{}/tokens_per_bin.png", &args.out_dir),
        )?;
        plot_fee_vs_vol(
            |va| fees.total_fee_rate(va),
            &format!("{}/fee_vs_volatility.png", &args.out_dir),
        )?;
    }
    Ok(())
}

fn write_schedule_csv_geometric(
    out_dir: &str,
    c: &Geometric,
    bins: i64,
    fees: DlmmFeeParams,
    va: f64,
    policy: &LaunchPhasePolicy,
    price_guard_bps: Option<f64>,
) -> Result<()> {
    let file_path = format!("{}/schedule.csv", out_dir);
    let mut file = File::create(&file_path)?;

    // Write metadata header
    writeln!(file, "# DLMM Bonding Curve Schedule")?;
    writeln!(file, "# Mode: Geometric, θ={}, R₀={}", c.theta, c.r0_quote)?;
    writeln!(
        file,
        "# Growth factor g={:.12}, Decay factor r={:.12}",
        c.g(),
        c.r()
    )?;
    writeln!(file, "# Volatility accumulator: {}", va)?;
    
    // Launch policy configuration
    writeln!(file, "# Launch policy: allowlist={} addresses", policy.allowlist.len())?;
    writeln!(
        file,
        "# Surcharge ramp: {:.1}% → {:.1}% over {:.0}s",
        policy.tau_start_pct, policy.tau_end_pct, policy.ramp_secs
    )?;

    // Optional price-guard metadata
    if let Some(impact_bps) = price_guard_bps {
        let bins_to_report = [0, bins / 2, bins.saturating_sub(1)];
        for b in bins_to_report {
            let p = c.price_of_bin(b);
            writeln!(file, "# Guard @ bin {} (P={:.12}):", b, p)?;
            writeln!(
                file,
                "#   Min X→Y: {:.12}",
                DlmmFeeParams::min_price_sell_x_for_y(p, impact_bps)
            )?;
            writeln!(
                file,
                "#   Min Y→X: {:.12}",
                DlmmFeeParams::min_price_sell_y_for_x(p, impact_bps)
            )?;
        }
    }
    writeln!(file)?;

    // Create CSV writer (write one header row)
    let mut wtr = csv::WriterBuilder::new().has_headers(false).from_writer(file);
    // explicit header
    wtr.write_record([
        "bin",
        "price",
        "delta_x",
        "supply_cum",
        "revenue_bin",
        "revenue_cum",
        "fee_base",
        "fee_var",
        "fee_total",
    ])?;

    // Neumaier compensated sums
    let mut s_cum = 0.0;
    let mut s_cmp = 0.0;
    let mut r_cum = 0.0;
    let mut r_cmp = 0.0;
    let fee_b = fees.base_fee_rate();
    let fee_v = fees.variable_fee_rate(va);
    let fee_tot = fees.total_fee_rate(va);

    for i in 0..bins {
        let p = c.price_of_bin(i);
        let dx = c.delta_x_of_bin(i);
        let r_bin = p * dx;
        // supply
        let t_s = s_cum + dx;
        if s_cum.abs() >= dx.abs() {
            s_cmp += (s_cum - t_s) + dx;
        } else {
            s_cmp += (dx - t_s) + s_cum;
        }
        s_cum = t_s;
        // revenue
        let t_r = r_cum + r_bin;
        if r_cum.abs() >= r_bin.abs() {
            r_cmp += (r_cum - t_r) + r_bin;
        } else {
            r_cmp += (r_bin - t_r) + r_cum;
        }
        r_cum = t_r;

        wtr.serialize(Row {
            bin: i,
            price: p,
            delta_x: dx,
            supply_cum: s_cum + s_cmp,
            revenue_bin: r_bin,
            revenue_cum: r_cum + r_cmp,
            fee_base: fee_b,
            fee_var: fee_v,
            fee_total: fee_tot,
        })?;
    }
    wtr.flush()?;
    Ok(())
}

fn run_logistic(
    args: &Args,
    grid: Grid,
    fees: DlmmFeeParams,
    policy: LaunchPhasePolicy,
) -> Result<()> {
    let p_max = args
        .p_max
        .ok_or_else(|| anyhow!("logistic: need --p-max"))?;
    if !(args.p_min < grid.p0 && grid.p0 < p_max) {
        return Err(anyhow!(
            "require p_min < p0 < p_max; got p_min={}, p0={}, p_max={}",
            args.p_min,
            grid.p0,
            p_max
        ));
    }
    let bins = if let Some(n) = args.bins {
        n
    } else if let Some(p_end) = args.end_price {
        if p_end <= grid.p0 {
            return Err(anyhow!(
                "logistic: require end_price > p0; got end_price={} ≤ p0={}",
                p_end,
                grid.p0
            ));
        }
        compute_bins_from_end_price(&grid, p_end)
    } else {
        500
    };

    let mut s_mid = args.s_mid;
    if s_mid == 0.0 {
        s_mid = ((p_max - grid.p0) / (grid.p0 - args.p_min)).ln() / args.k;
    }
    let curve = LogisticS {
        grid,
        p_min: args.p_min,
        p_max,
        k: args.k,
        s_mid,
        bins,
    };
    if args.verbose {
        println!(
            "[{}] bins={} p_min={:.6} p_max={:.6} k={:.8} s_mid={:.2}",
            curve.name(),
            bins,
            args.p_min,
            p_max,
            args.k,
            s_mid
        );
        println!(
            "  Cumulative supply at n={}: {:.6}",
            bins,
            curve.cumulative_supply(bins)
        );
        println!("  Allowlist size: {}", policy.allowlist.len());
        println!(
            "  Launch surcharge: τ(0s)={:.1}% → τ({:.0}s)={:.1}%",
            policy.tau(0.0),
            policy.ramp_secs,
            policy.tau(policy.ramp_secs)
        );
    }

    write_schedule_csv_generic(
        &args.out_dir,
        &curve,
        bins,
        fees,
        args.vol_accum,
        &policy,
        args.price_guard_bps,
    )?;
    if args.draw {
        plot_price_vs_supply(
            &curve,
            bins,
            &format!("{}/price_vs_supply.png", &args.out_dir),
        )?;
        plot_tokens_per_bin(
            &curve,
            bins,
            &format!("{}/tokens_per_bin.png", &args.out_dir),
        )?;
        plot_fee_vs_vol(
            |va| fees.total_fee_rate(va),
            &format!("{}/fee_vs_volatility.png", &args.out_dir),
        )?;
    }
    Ok(())
}

fn write_schedule_csv_generic<C: Curve>(
    out_dir: &str,
    c: &C,
    bins: i64,
    fees: DlmmFeeParams,
    va: f64,
    policy: &LaunchPhasePolicy,
    price_guard_bps: Option<f64>,
) -> Result<()> {
    let file_path = format!("{}/schedule.csv", out_dir);
    let mut file = File::create(&file_path)?;

    // Write metadata header
    writeln!(file, "# DLMM Bonding Curve Schedule")?;
    writeln!(file, "# Mode: {}", c.name())?;
    writeln!(file, "# Volatility accumulator: {}", va)?;
    writeln!(file, "# Total supply: {:.6}", c.cumulative_supply(bins))?;
    
    // Launch policy configuration
    writeln!(file, "# Launch policy: allowlist={} addresses", policy.allowlist.len())?;
    writeln!(
        file,
        "# Surcharge ramp: {:.1}% → {:.1}% over {:.0}s",
        policy.tau_start_pct, policy.tau_end_pct, policy.ramp_secs
    )?;

    // Optional price-guard metadata
    if let Some(impact_bps) = price_guard_bps {
        for (label, bin) in [("start", 0), ("mid", bins / 2), ("end", bins.saturating_sub(1))] {
            let price = c.price_of_bin(bin);
            writeln!(file, "# Guard @ {} (bin {}, P={:.12}):", label, bin, price)?;
            writeln!(
                file,
                "#   Min X→Y: {:.12}",
                DlmmFeeParams::min_price_sell_x_for_y(price, impact_bps)
            )?;
            writeln!(
                file,
                "#   Min Y→X: {:.12}",
                DlmmFeeParams::min_price_sell_y_for_x(price, impact_bps)
            )?;
        }
    }

    writeln!(file)?;

    // Create CSV writer (write one header row)
    let mut wtr = csv::WriterBuilder::new().has_headers(false).from_writer(file);
    // explicit header
    wtr.write_record([
        "bin",
        "price",
        "delta_x",
        "supply_cum",
        "revenue_bin",
        "revenue_cum",
        "fee_base",
        "fee_var",
        "fee_total",
    ])?;

    // Neumaier compensated sums
    let mut s_cum = 0.0;
    let mut s_cmp = 0.0;
    let mut r_cum = 0.0;
    let mut r_cmp = 0.0;
    let fee_b = fees.base_fee_rate();
    let fee_v = fees.variable_fee_rate(va);
    let fee_tot = fees.total_fee_rate(va);

    for i in 0..bins {
        let p = c.price_of_bin(i);
        let dx = c.delta_x_of_bin(i);
        let r_bin = p * dx;
        // supply
        let t_s = s_cum + dx;
        if s_cum.abs() >= dx.abs() {
            s_cmp += (s_cum - t_s) + dx;
        } else {
            s_cmp += (dx - t_s) + s_cum;
        }
        s_cum = t_s;
        // revenue
        let t_r = r_cum + r_bin;
        if r_cum.abs() >= r_bin.abs() {
            r_cmp += (r_cum - t_r) + r_bin;
        } else {
            r_cmp += (r_bin - t_r) + r_cum;
        }
        r_cum = t_r;

        wtr.serialize(Row {
            bin: i,
            price: p,
            delta_x: dx,
            supply_cum: s_cum + s_cmp,
            revenue_bin: r_bin,
            revenue_cum: r_cum + r_cmp,
            fee_base: fee_b,
            fee_var: fee_v,
            fee_total: fee_tot,
        })?;
    }
    wtr.flush()?;
    Ok(())
}
