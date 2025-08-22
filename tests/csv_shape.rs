use bcurve::dlmm::LaunchPhasePolicy;
use std::collections::HashSet;
use std::fs;
use std::process::Command;

#[test]
fn schedule_has_one_header_and_no_demo_cols() {
    let out = "out_shape_test";
    let status = Command::new("./target/release/bcurve")
        .args([
            "--mode",
            "geometric",
            "--p0",
            "0.01",
            "--bin-step-bps",
            "10",
            "--theta",
            "0.6",
            "--bins",
            "5",
            "--r0",
            "100.0",
            "--out-dir",
            out,
            "--no-draw",
        ])
        .status()
        .expect("run bcurve");
    assert!(status.success());

    let s = fs::read_to_string(format!("{out}/schedule.csv")).unwrap();
    let header_count = s
        .lines()
        .filter(|l| l.starts_with("bin,price,delta_x"))
        .count();
    assert_eq!(header_count, 1, "CSV must have exactly one header row");

    assert!(
        !s.contains("surcharge_launch_pct"),
        "no example surcharge columns"
    );
    assert!(
        !s.contains("fee_total_plus_surcharge"),
        "no example surcharge columns"
    );

    // Verify correct column structure
    let header_line = s
        .lines()
        .find(|l| l.starts_with("bin,"))
        .expect("should have header line");
    assert_eq!(
        header_line,
        "bin,price,delta_x,supply_cum,revenue_bin,revenue_cum,fee_base,fee_var,fee_total"
    );

    // Clean up
    let _ = fs::remove_dir_all(out);
}

#[test]
fn compensated_summation_accuracy() {
    let out = "out_accuracy_test";
    let status = Command::new("./target/release/bcurve")
        .args([
            "--mode",
            "geometric",
            "--p0",
            "0.01",
            "--bin-step-bps",
            "10",
            "--theta",
            "0.6",
            "--bins",
            "100",
            "--r0",
            "100.0",
            "--out-dir",
            out,
            "--no-draw",
        ])
        .status()
        .expect("run bcurve");
    assert!(status.success());

    let s = fs::read_to_string(format!("{out}/schedule.csv")).unwrap();

    // Parse the last data row to check cumulative accuracy
    let last_data_line = s
        .lines()
        .filter(|l| !l.starts_with("#") && !l.starts_with("bin,"))
        .last()
        .expect("should have data rows");

    let fields: Vec<&str> = last_data_line.split(',').collect();
    let supply_cum: f64 = fields[3].parse().expect("should parse supply_cum");
    let revenue_cum: f64 = fields[5].parse().expect("should parse revenue_cum");

    // These should be finite numbers (compensated summation prevents overflow/underflow)
    assert!(supply_cum.is_finite(), "supply_cum should be finite");
    assert!(revenue_cum.is_finite(), "revenue_cum should be finite");
    assert!(supply_cum > 0.0, "supply_cum should be positive");
    assert!(revenue_cum > 0.0, "revenue_cum should be positive");

    // Clean up
    let _ = fs::remove_dir_all(out);
}

#[test]
fn launch_phase_policy_allowlist_functionality() {
    let mut allowlist = HashSet::new();
    allowlist.insert("privileged_trader_123".to_string());
    allowlist.insert("whale_address_456".to_string());
    allowlist.insert("team_member_789".to_string());

    let policy = LaunchPhasePolicy {
        allowlist,
        tau_start_pct: 50.0,
        tau_end_pct: 5.0,
        ramp_secs: 120.0,
    };

    // Test allowlisted addresses
    assert!(policy.is_allowed("privileged_trader_123"));
    assert!(policy.is_allowed("whale_address_456"));
    assert!(policy.is_allowed("team_member_789"));

    // Test non-allowlisted addresses
    assert!(!policy.is_allowed("regular_user_abc"));
    assert!(!policy.is_allowed("unknown_address"));
    assert!(!policy.is_allowed(""));

    // Test case sensitivity
    assert!(!policy.is_allowed("PRIVILEGED_TRADER_123"));
    assert!(!policy.is_allowed("privileged_trader_123 "));

    // Test allowlist size
    assert_eq!(policy.allowlist.len(), 3);

    // Test tau function works correctly
    assert_eq!(policy.tau(0.0), 50.0);
    assert_eq!(policy.tau(120.0), 5.0);
    assert_eq!(policy.tau(60.0), 27.5); // midpoint
}
