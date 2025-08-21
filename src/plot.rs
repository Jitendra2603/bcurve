//! Visualization utilities for generating charts

use crate::curves::Curve;
use anyhow::Result;
use plotters::prelude::*;

/// Generates a price vs cumulative supply chart
pub fn plot_price_vs_supply<C: Curve>(c: &C, bins: i64, out_path: &str) -> Result<()> {
    let root = BitMapBackend::new(out_path, (1200, 700)).into_drawing_area();
    root.fill(&WHITE)?;
    let mut supply = 0.0_f64;
    let mut data: Vec<(f64, f64)> = Vec::with_capacity(2 * bins as usize);
    for i in 0..bins {
        let p = c.price_of_bin(i);
        data.push((supply, p));
        supply += c.delta_x_of_bin(i);
        data.push((supply, p)); // step
    }
    let x_max = data.last().map(|(x, _)| *x).unwrap_or(1.0).max(1e-12);
    let y_max = data.iter().map(|(_, y)| *y).fold(0.0, f64::max).max(1e-12);
    let mut chart = ChartBuilder::on(&root)
        .margin(20)
        .caption("Price vs Cumulative Supply", ("sans-serif", 28))
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0..x_max, 0.0..(y_max * 1.05))?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(data, &BLACK))?;
    root.present()?;
    Ok(())
}

/// Generates a line chart showing token distribution across bins
pub fn plot_tokens_per_bin<C: Curve>(c: &C, bins: i64, out_path: &str) -> Result<()> {
    let root = BitMapBackend::new(out_path, (1200, 700)).into_drawing_area();
    root.fill(&WHITE)?;
    let pts: Vec<(f64, f64)> = (0..bins).map(|i| (i as f64, c.delta_x_of_bin(i))).collect();
    let x_max = (bins as f64).max(1.0);
    let y_max = pts.iter().map(|(_, y)| *y).fold(0.0, f64::max).max(1e-12);
    let mut chart = ChartBuilder::on(&root)
        .margin(20)
        .caption("Tokens per Bin (ΔX_i)", ("sans-serif", 28))
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0..x_max, 0.0..(y_max * 1.05))?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(pts, &BLACK))?;
    root.present()?;
    Ok(())
}

/// Generates a chart showing fee rate as a function of volatility accumulator
pub fn plot_fee_vs_vol(compute_fee: impl Fn(f64) -> f64, out_path: &str) -> Result<()> {
    let root = BitMapBackend::new(out_path, (1200, 700)).into_drawing_area();
    root.fill(&WHITE)?;
    let pts: Vec<(f64, f64)> = (0..=500)
        .map(|v| {
            let va = v as f64 / 10.0;
            (va, compute_fee(va))
        })
        .collect();
    let x_max = 50.0;
    let y_max = pts.iter().map(|(_, y)| *y).fold(0.0, f64::max).max(1e-12);
    let mut chart = ChartBuilder::on(&root)
        .margin(20)
        .caption("Total Fee vs Volatility Accumulator", ("sans-serif", 28))
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0..x_max, 0.0..(y_max * 1.05))?;
    chart.configure_mesh().draw()?;
    chart.draw_series(LineSeries::new(pts, &BLACK))?;
    root.present()?;
    Ok(())
}
