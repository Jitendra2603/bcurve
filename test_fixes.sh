#!/bin/bash

echo "Testing DLMM Bonding Curve Verifier with surgical fixes"
echo "======================================================="

# Geometric with proper fee calculation
echo -e "\n=== Test 1: Geometric with variable fee control ==="
./target/release/bcurve \
  --mode geometric \
  --p0 0.01 --bin-step-bps 10 \
  --theta 0.6 \
  --target-supply 100000000 \
  --end-price 0.0175 \
  --base-factor 0.0 \
  --variable-fee-control 0.05 \
  --vol-accum 10 \
  --max-fee-rate 0.10 \
  --verbose true \
  --out-dir test_geometric_fees

echo -e "\nChecking fee calculation in CSV (should show non-zero fee with VA=10):"
head -5 test_geometric_fees/schedule.csv | tail -1

# Logistic with domain safety
echo -e "\n\n=== Test 2: Logistic S-curve with domain validation ==="
./target/release/bcurve \
  --mode logistic \
  --p0 0.0015 --bin-step-bps 10 \
  --p-min 0.001 --p-max 0.05 \
  --k 0.00000008 \
  --end-price 0.03 \
  --vol-accum 5.0 \
  --verbose true \
  --out-dir test_logistic_safe

# Edge case; should fail with proper error
echo -e "\n\n=== Test 3: Invalid logistic parameters (should fail gracefully) ==="
./target/release/bcurve \
  --mode logistic \
  --p0 0.001 --bin-step-bps 10 \
  --p-min 0.002 --p-max 0.05 \
  --k 0.00000008 \
  --verbose true \
  --out-dir test_logistic_fail 2>&1 | grep "Require p_min < p0 < p_max"

echo -e "\n\nTests complete!"
