use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::terlan_quality::lean_proof_track::lean_proof_gap::{blocker_hash, GAP_HEADER};

/// Verifies unchanged Lean proof metrics pass and archive a report.
///
/// Inputs:
/// - Temporary Lean baseline, inventory, gap manifest, and regression policy.
///
/// Output:
/// - Summary with one feature class and a generated JSON report.
///
/// Transformation:
/// - Proves the gate can compare current proof debt against a baseline without
///   requiring an active Lean tree.
#[test]
fn lean_proof_regression_accepts_unchanged_baseline() {
    let root = temp_repo("lean_proof_regression_accepts");
    write_fixture(&root, baseline_one_row("Core preservation", 0, 0, 1));
    write_inventory(&root, "");
    write_gaps(&root, "Core preservation\tmissing proof\tcompiler\tlean-proof-track-check\tdocs/compiler/LEAN_PROOF_TRACK.md\n");
    write_policy(&root, "Core preservation");

    let summary = run_lean_proof_regression(&root).expect("unchanged baseline should pass");

    assert_eq!(summary.feature_class_count, 1);
    assert_eq!(summary.warning_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("\"feature_class\": \"Core preservation\""));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies proof-count drops fail.
///
/// Inputs:
/// - Baseline requiring one current proof row.
/// - Current inventory with no current proof rows.
///
/// Output:
/// - Diagnostic naming the proof-count drop.
///
/// Transformation:
/// - Prevents deleting proof coverage while leaving the baseline stale.
#[test]
fn lean_proof_regression_rejects_proof_count_drop() {
    let root = temp_repo("lean_proof_regression_proof_drop");
    write_fixture(&root, baseline_one_row("Core preservation", 1, 0, 1));
    write_inventory(&root, "");
    write_gaps(&root, "Core preservation\tmissing proof\tcompiler\tlean-proof-track-check\tdocs/compiler/LEAN_PROOF_TRACK.md\n");
    write_policy(&root, "Core preservation");

    let error = run_lean_proof_regression(&root).expect_err("proof drop should fail");

    assert!(error.contains("Core preservation: proof count dropped from 1 to 0"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies malformed policy thresholds fail.
///
/// Inputs:
/// - Policy whose warning threshold is higher than hard threshold.
///
/// Output:
/// - Diagnostic naming the threshold ordering rule.
///
/// Transformation:
/// - Keeps warning and hard-failure promotion deterministic.
#[test]
fn lean_proof_regression_rejects_bad_policy_threshold_order() {
    let policy = Policy {
        warning_threshold_percent: 20,
        hard_threshold_percent: 10,
        monotonic_lane: "Core preservation".to_string(),
    };

    let diagnostics = validate_policy(&policy);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("warning threshold must be <= hard threshold")));
}

/// Verifies baseline percentages are bounded.
///
/// Inputs:
/// - A baseline row with an impossible pass rate.
///
/// Output:
/// - Parse error naming the invalid percentage.
///
/// Transformation:
/// - Prevents trend reports from accepting impossible health metrics.
#[test]
fn lean_proof_regression_rejects_invalid_percent() {
    let error = parse_metrics(
        "feature_class\tcurrent_proof_count\tstale_count\tgap_count\tnondeterministic_count\trepro_fail_count\trepro_pass_rate_7d\tlane_pass_rate_30d\n\
         Core preservation\t0\t0\t1\t0\t0\t101\t100\n",
    )
    .expect_err("invalid percent should fail");

    assert!(error.contains("percentage `101` must be <= 100"));
}

fn temp_repo(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(root.join("docs/compiler/proof_track")).expect("create proof docs");
    fs::create_dir_all(root.join("docs/runtime")).expect("create runtime docs");
    root
}

fn write_fixture(root: &Path, baseline: String) {
    fs::write(root.join(BASELINE_PATH), baseline).expect("write baseline");
    fs::write(
        root.join("docs/compiler/LEAN_PROOF_TRACK.md"),
        "proof track",
    )
    .expect("write doc");
}

fn write_inventory(root: &Path, rows: &str) {
    fs::write(
        root.join(INVENTORY_PATH),
        format!("{INVENTORY_HEADER}\n{rows}"),
    )
    .expect("write inventory");
}

fn write_gaps(root: &Path, rows: &str) {
    let rows = rows
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 5, "legacy test gap row: {line}");
            let updated_at = "2026-07-16";
            format!(
                "{}\tblocked\tmodel_gap\t{}\t{}\t{}\tdeadline:0.0.7-closeout\t{updated_at}\t{}\t{}",
                columns[0],
                columns[1],
                columns[2],
                columns[3],
                blocker_hash(columns[0], "model_gap", columns[1], updated_at),
                columns[4]
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join(GAP_PATH), format!("{GAP_HEADER}\n{rows}")).expect("write gaps");
}

fn write_policy(root: &Path, monotonic_lane: &str) {
    fs::write(
        root.join(POLICY_PATH),
        format!(
            "{POLICY_HEADER}\nwarning_threshold_percent\t5\nhard_threshold_percent\t10\nmonotonic_lane\t{monotonic_lane}\n"
        ),
    )
    .expect("write policy");
}

fn baseline_one_row(feature: &str, current: u64, stale: u64, gaps: u64) -> String {
    format!("{BASELINE_HEADER}\n{feature}\t{current}\t{stale}\t{gaps}\t0\t0\t100\t100\n")
}
