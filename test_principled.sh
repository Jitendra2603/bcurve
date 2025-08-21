#!/bin/bash

echo "Testing Principled DLMM Bonding Curve"
echo "=============================================="

# Geometric with proper decimal fees
echo -e "\n=== Test 1: Geometric with decimal fee model ==="
./target/release/bcurve \
  --mode geometric \
  --p0 0.01 --bin-step-bps 10 \
  --theta 0.6 \
  --target-supply 100000000 \
  --end-price 0.0175 \
  --base-factor 0.3 \
  --variable-fee-control 0.0001 \
  --vol-accum 10 \
  --max-fee-rate 0.05 \
  --verbose true \
  --out-dir test_geometric_decimal

echo -e "\nFee calculation (should be ~0.0003 base + variable with VA=10):"
head -5 test_geometric_decimal/schedule.csv | tail -1

# Logistic with domain validation
echo -e "\n\n=== Test 2: Logistic S-curve with proper guards ==="
./target/release/bcurve \
  --mode logistic \
  --p0 0.0015 --bin-step-bps 10 \
  --p-min 0.001 --p-max 0.05 \
  --k 0.00000008 \
  --end-price 0.03 \
  --vol-accum 5.0 \
  --verbose true \
  --out-dir test_logistic_principled

echo -e "\n\n=== Test 3: Running property tests ==="
cargo test --release -- --nocapture geometric_closed_form_matches_sum

echo -e "\n\nAll tests complete!"
