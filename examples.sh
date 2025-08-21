#!/bin/bash

# DLMM Bonding Curve 

echo "Building the project..."
cargo build --release

echo -e "\n\n=== Example 1: Geometric Curve (DLMM-native) ==="
echo "Solving for R0 with target supply 100M tokens, θ=0.6"
./target/release/bcurve \
  --mode geometric \
  --p0 0.01 --bin-step-bps 10 \
  --theta 0.6 \
  --target-supply 100000000 \
  --end-price 0.0175 \
  --verbose true \
  --out-dir out_example1_geo

echo -e "\n\n=== Example 2: Logistic S-curve ==="
echo "Creating S-curve with P_min=0.001, P_max=0.05"
./target/release/dlmm_bonding_curve_verifier \
  --mode logistic \
  --p0 0.0015 --bin-step-bps 10 \
  --p-min 0.001 --p-max 0.05 \
  --k 0.00000008 \
  --end-price 0.03 \
  --verbose true \
  --out-dir out_example2_logistic

echo -e "\n\n=== Example 3: With Fees ==="
echo "Geometric curve with DLMM fee model"
./target/release/bcurve \
  --mode geometric \
  --p0 0.01 --bin-step-bps 10 \
  --theta 0.6 \
  --target-supply 50000000 \
  --end-price 0.02 \
  --base-factor 0.3 \
  --variable-fee-control 0.0001 \
  --vol-accum 10.0 \
  --verbose true \
  --out-dir out_example3_fees

echo -e "\n\n=== Example 4: Higher θ schedule ==="
echo "Steeper curve with θ=0.8"
./target/release/bcurve \
  --mode geometric \
  --p0 0.001 --bin-step-bps 20 \
  --theta 0.8 \
  --target-supply 1000000000 \
  --end-price 0.05 \
  --tau-start-pct 75.0 \
  --tau-end-pct 5.0 \
  --tau-ramp-secs 60.0 \
  --verbose true \
  --out-dir out_example4_pump

echo -e "\n\nAll examples complete! Check the output directories for:"
echo "  - schedule.csv: Per-bin token distribution"
echo "  - price_vs_supply.png: Price curve visualization"
echo "  - tokens_per_bin.png: Token distribution across bins"
echo "  - fee_vs_volatility.png: Fee model visualization"
