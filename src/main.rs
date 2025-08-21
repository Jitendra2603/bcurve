mod curves;
mod dlmm;
mod plot;
mod verifier;

use crate::curves::{Curve, Geometric, Grid, LogisticS};
use crate::dlmm::{DlmmFeeParams, LaunchPhaseSurcharge};
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

    // legacy integer knobs (kept but unused unless you switch to integer mode later)
    #[arg(long, default_value_t = 99_999_999_999.0)]
    fee_offset: f64,
    #[arg(long, default_value_t = 100_000_000_000.0)]
    fee_scale: f64,

    // Transient surcharge
    #[arg(long, default_value_t = 50.0)]
    tau_start_pct: f64,
    #[arg(long, default_value_t = 3.0)]
    tau_end_pct: f64,
    #[arg(long, default_value_t = 30.0)]
    tau_ramp_secs: f64,
    #[arg(long)]
    whitelist_path: Option<String>,

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
    surcharge_launch_pct: f64,
    fee_total_plus_surcharge: f64,
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
    if args.max_fee_rate < 0.0 {
        return Err(anyhow!(
            "max_fee_rate must be ≥ 0 (got {})",
            args.max_fee_rate
        ));
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

    let mut whitelist = HashSet::new();
    if let Some(path) = &args.whitelist_path {
        if Path::new(path).exists() {
            for line in std::fs::read_to_string(path)?.lines() {
                let addr = line.trim();
                if !addr.is_empty() {
                    whitelist.insert(addr.to_string());
                }
            }
        }
    }
    let surcharge = LaunchPhaseSurcharge {
        whitelist,
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
        fee_offset: args.fee_offset,
        fee_scale: args.fee_scale,
    };

    create_dir_all(&args.out_dir)?;

    match args.mode.as_str() {
        "geometric" => run_geometric(&args, grid, fees, surcharge),
        "logistic" => run_logistic(&args, grid, fees, surcharge),
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
    surcharge: LaunchPhaseSurcharge,
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
            "  Growth factor g=q^θ={:.6}, Decay factor r=q^(θ-1)={:.6}",
            curve.g(),
            curve.r()
        );
        println!(
            "  Cumulative supply at n={}: {:.6}",
            bins,
            curve.cumulative_supply(bins)
        );
        println!("  Whitelist addresses: {}", surcharge.whitelist.len());

        // Show surcharge behavior
        for t in [0.0, 15.0, 30.0, 60.0] {
            println!("  Launch surcharge at t={}s: {:.1}%", t, surcharge.tau(t));
        }

        // Example: check if an address is whitelisted
        if surcharge.whitelist.len() > 0 {
            let example_addr = surcharge.whitelist.iter().next().unwrap();
            println!(
                "  Example: {} is whitelisted: {}",
                example_addr,
                surcharge.is_whitelisted(example_addr)
            );
        }
    }

    write_schedule_csv_geometric(
        &args.out_dir,
        &curve,
        bins,
        fees,
        args.vol_accum,
        &surcharge,
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
    surcharge: &LaunchPhaseSurcharge,
) -> Result<()> {
    let file_path = format!("{}/schedule.csv", out_dir);
    let mut file = File::create(&file_path)?;

    // Write metadata header
    writeln!(file, "# DLMM Bonding Curve Schedule")?;
    writeln!(file, "# Mode: Geometric, θ={}, R₀={}", c.theta, c.r0_quote)?;
    writeln!(
        file,
        "# Growth factor g={:.6}, Decay factor r={:.6}",
        c.g(),
        c.r()
    )?;
    writeln!(file, "# Volatility accumulator: {}", va)?;

    // Price impact guards (example calculation at mid-point)
    let mid_bin = bins / 2;
    let mid_price = c.price_of_bin(mid_bin);
    let impact_bps = 50.0; // 0.5% example
    writeln!(
        file,
        "# Price guards at bin {} (P={:.6}):",
        mid_bin, mid_price
    )?;
    writeln!(
        file,
        "#   Min price X→Y: {:.6}",
        DlmmFeeParams::min_price_sell_x_for_y(mid_price, impact_bps)
    )?;
    writeln!(
        file,
        "#   Min price Y→X: {:.6}",
        DlmmFeeParams::min_price_sell_y_for_x(mid_price, impact_bps)
    )?;
    writeln!(file, "")?;

    // Create CSV writer from the file
    let mut wtr = csv::Writer::from_writer(file);
    // explicit header
    wtr.write_record(&[
        "bin",
        "price",
        "delta_x",
        "supply_cum",
        "revenue_bin",
        "revenue_cum",
        "fee_base",
        "fee_var",
        "fee_total",
        "surcharge_launch_pct",
        "fee_total_plus_surcharge",
    ])?;

    let mut s_cum = 0.0;
    let mut r_cum = 0.0;
    let fee_b = fees.base_fee_rate();
    let fee_v = fees.variable_fee_rate(va);
    let fee_tot = fees.total_fee_rate(va);
    let tau0 = surcharge.tau(0.0); // %
    let tau0_dec = tau0 / 100.0;

    for i in 0..bins {
        let p = c.price_of_bin(i);
        let dx = c.delta_x_of_bin(i);
        let r_bin = p * dx;
        s_cum += dx;
        r_cum += r_bin;

        wtr.serialize(Row {
            bin: i,
            price: p,
            delta_x: dx,
            supply_cum: s_cum,
            revenue_bin: r_bin,
            revenue_cum: r_cum,
            fee_base: fee_b,
            fee_var: fee_v,
            fee_total: fee_tot,
            surcharge_launch_pct: tau0,
            fee_total_plus_surcharge: fee_tot + tau0_dec,
        })?;
    }
    wtr.flush()?;
    Ok(())
}

fn run_logistic(
    args: &Args,
    grid: Grid,
    fees: DlmmFeeParams,
    surcharge: LaunchPhaseSurcharge,
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
        println!("  Whitelist addresses: {}", surcharge.whitelist.len());

        // Show surcharge behavior
        for t in [0.0, 15.0, 30.0, 60.0] {
            println!("  Launch surcharge at t={}s: {:.1}%", t, surcharge.tau(t));
        }
    }

    write_schedule_csv_generic(
        &args.out_dir,
        &curve,
        bins,
        fees,
        args.vol_accum,
        &surcharge,
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
    surcharge: &LaunchPhaseSurcharge,
) -> Result<()> {
    let file_path = format!("{}/schedule.csv", out_dir);
    let mut file = File::create(&file_path)?;

    // Write metadata header
    writeln!(file, "# DLMM Bonding Curve Schedule")?;
    writeln!(file, "# Mode: {}", c.name())?;
    writeln!(file, "# Volatility accumulator: {}", va)?;
    writeln!(file, "# Total supply: {:.6}", c.cumulative_supply(bins))?;

    // Price impact guards at several points
    for (label, bin) in [("start", 0), ("mid", bins / 2), ("end", bins - 1)].iter() {
        let price = c.price_of_bin(*bin);
        let impact_bps = 50.0; // 0.5% example
        writeln!(
            file,
            "# Price guards at {} (bin {}, P={:.6}):",
            label, bin, price
        )?;
        writeln!(
            file,
            "#   Min price X→Y: {:.6}",
            DlmmFeeParams::min_price_sell_x_for_y(price, impact_bps)
        )?;
        writeln!(
            file,
            "#   Min price Y→X: {:.6}",
            DlmmFeeParams::min_price_sell_y_for_x(price, impact_bps)
        )?;
    }

    // Show surcharge decay
    writeln!(
        file,
        "# Launch surcharge: {:.1}% → {:.1}% over {} seconds",
        surcharge.tau_start_pct, surcharge.tau_end_pct, surcharge.ramp_secs
    )?;
    if surcharge.whitelist.len() > 0 {
        writeln!(
            file,
            "# Whitelisted addresses: {}",
            surcharge.whitelist.len()
        )?;
        for addr in surcharge.whitelist.iter().take(3) {
            writeln!(file, "#   - {}", addr)?;
        }
        if surcharge.whitelist.len() > 3 {
            writeln!(file, "#   ... and {} more", surcharge.whitelist.len() - 3)?;
        }
    }
    writeln!(file, "")?;

    // Create CSV writer from the file
    let mut wtr = csv::Writer::from_writer(file);
    // explicit header
    wtr.write_record(&[
        "bin",
        "price",
        "delta_x",
        "supply_cum",
        "revenue_bin",
        "revenue_cum",
        "fee_base",
        "fee_var",
        "fee_total",
        "surcharge_launch_pct",
        "fee_total_plus_surcharge",
    ])?;

    let mut s_cum = 0.0;
    let mut r_cum = 0.0;
    let fee_b = fees.base_fee_rate();
    let fee_v = fees.variable_fee_rate(va);
    let fee_tot = fees.total_fee_rate(va);
    let tau0 = surcharge.tau(0.0); // %
    let tau0_dec = tau0 / 100.0;

    for i in 0..bins {
        let p = c.price_of_bin(i);
        let dx = c.delta_x_of_bin(i);
        let r_bin = p * dx;
        s_cum += dx;
        r_cum += r_bin;

        wtr.serialize(Row {
            bin: i,
            price: p,
            delta_x: dx,
            supply_cum: s_cum,
            revenue_bin: r_bin,
            revenue_cum: r_cum,
            fee_base: fee_b,
            fee_var: fee_v,
            fee_total: fee_tot,
            surcharge_launch_pct: tau0,
            fee_total_plus_surcharge: fee_tot + tau0_dec,
        })?;
    }
    wtr.flush()?;
    Ok(())
}
