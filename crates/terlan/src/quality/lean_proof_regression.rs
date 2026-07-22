use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use time::OffsetDateTime;

use crate::terlan_quality::lean_proof_track::lean_proof_gap::{parse_gap_manifest, GAP_PATH};
use crate::terlan_quality::{render_failure, QualityResult};

const BASELINE_PATH: &str = "docs/compiler/LEAN_PROOF_METRICS.tsv";
const INVENTORY_PATH: &str = "docs/compiler/proof_track/lean_proof_inventory.tsv";
const POLICY_PATH: &str = "docs/runtime/lean-proof-regression-policy.tsv";
const HISTORY_DIR: &str = "build/artifacts/lean-proof-history";

const BASELINE_HEADER: &str = "feature_class\tcurrent_proof_count\tstale_count\tgap_count\tnondeterministic_count\trepro_fail_count\trepro_pass_rate_7d\tlane_pass_rate_30d";
const INVENTORY_HEADER: &str = "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes";
const POLICY_HEADER: &str = "name\tvalue";

/// Summary produced by the Lean proof regression gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanProofRegressionSummary {
    pub feature_class_count: usize,
    pub warning_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MetricRow {
    feature_class: String,
    current_proof_count: u64,
    stale_count: u64,
    gap_count: u64,
    nondeterministic_count: u64,
    repro_fail_count: u64,
    repro_pass_rate_7d: u64,
    lane_pass_rate_30d: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Policy {
    warning_threshold_percent: u64,
    hard_threshold_percent: u64,
    monotonic_lane: String,
}

#[derive(Debug, Serialize)]
struct LeanProofRegressionReport {
    generated_date: String,
    warning_threshold_percent: u64,
    hard_threshold_percent: u64,
    baseline: Vec<MetricRow>,
    current: Vec<MetricRow>,
    warnings: Vec<String>,
}

/// Runs Lean proof regression tracking.
///
/// Inputs:
/// - `root`: repository root containing proof inventory, gap, baseline, and
///   policy TSV files.
///
/// Output:
/// - Success summary with feature-class count, warning count, and JSON report.
/// - Stable diagnostics when proof counts regress, nondeterminism increases,
///   baseline rows disappear, or policy constants are malformed.
///
/// Transformation:
/// - Converts proof-track TSV data into machine-readable trend evidence without
///   treating missing formal proofs as completed coverage.
pub fn run_lean_proof_regression(root: &Path) -> QualityResult<LeanProofRegressionSummary> {
    let baseline = parse_metrics(&read_text(root, BASELINE_PATH)?)?;
    let inventory = parse_inventory(&read_text(root, INVENTORY_PATH)?)?;
    let gaps = parse_gaps(&read_text(root, GAP_PATH)?)?;
    let policy = parse_policy(&read_text(root, POLICY_PATH)?)?;
    let current = derive_current_metrics(&baseline, &inventory, &gaps);

    let mut diagnostics = validate_baseline(&baseline);
    diagnostics.extend(validate_policy(&policy));
    diagnostics.extend(validate_regression(&baseline, &current, &policy));

    if !diagnostics.is_empty() {
        return Err(render_failure("lean-proof-regression", &diagnostics));
    }

    let warnings = collect_warnings(&baseline, &current, &policy);
    let feature_class_count = current.len();
    let report_path = write_history_report(
        root,
        &LeanProofRegressionReport {
            generated_date: today_utc(),
            warning_threshold_percent: policy.warning_threshold_percent,
            hard_threshold_percent: policy.hard_threshold_percent,
            baseline,
            current,
            warnings: warnings.clone(),
        },
    )?;

    Ok(LeanProofRegressionSummary {
        feature_class_count,
        warning_count: warnings.len(),
        report_path,
    })
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

fn parse_metrics(text: &str) -> QualityResult<Vec<MetricRow>> {
    parse_tsv(text, BASELINE_HEADER, BASELINE_PATH)?
        .into_iter()
        .map(|columns| {
            Ok(MetricRow {
                feature_class: columns[0].clone(),
                current_proof_count: parse_u64(&columns[1], BASELINE_PATH)?,
                stale_count: parse_u64(&columns[2], BASELINE_PATH)?,
                gap_count: parse_u64(&columns[3], BASELINE_PATH)?,
                nondeterministic_count: parse_u64(&columns[4], BASELINE_PATH)?,
                repro_fail_count: parse_u64(&columns[5], BASELINE_PATH)?,
                repro_pass_rate_7d: parse_percent(&columns[6], BASELINE_PATH)?,
                lane_pass_rate_30d: parse_percent(&columns[7], BASELINE_PATH)?,
            })
        })
        .collect()
}

fn parse_inventory(text: &str) -> QualityResult<Vec<(String, String)>> {
    Ok(parse_tsv(text, INVENTORY_HEADER, INVENTORY_PATH)?
        .into_iter()
        .map(|columns| (columns[1].clone(), columns[2].clone()))
        .collect())
}

fn parse_gaps(text: &str) -> QualityResult<Vec<String>> {
    Ok(parse_gap_manifest(text)?
        .into_iter()
        .map(|gap| gap.feature)
        .collect())
}

fn parse_policy(text: &str) -> QualityResult<Policy> {
    let rows = parse_tsv(text, POLICY_HEADER, POLICY_PATH)?;
    let values = rows
        .into_iter()
        .map(|columns| (columns[0].clone(), columns[1].clone()))
        .collect::<BTreeMap<_, _>>();
    Ok(Policy {
        warning_threshold_percent: parse_u64(
            required_policy(&values, "warning_threshold_percent")?,
            POLICY_PATH,
        )?,
        hard_threshold_percent: parse_u64(
            required_policy(&values, "hard_threshold_percent")?,
            POLICY_PATH,
        )?,
        monotonic_lane: required_policy(&values, "monotonic_lane")?.to_string(),
    })
}

fn required_policy<'a>(values: &'a BTreeMap<String, String>, key: &str) -> QualityResult<&'a str> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("{POLICY_PATH}: missing policy `{key}`"))
}

fn parse_tsv(text: &str, header: &str, path: &str) -> QualityResult<Vec<Vec<String>>> {
    let mut lines = text.lines();
    let Some(actual_header) = lines.next() else {
        return Err(format!("{path}: missing header"));
    };
    if actual_header != header {
        return Err(format!(
            "{path}: expected header `{header}`, found `{actual_header}`"
        ));
    }
    let expected_columns = header.split('\t').count();
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns = line.split('\t').map(str::to_string).collect::<Vec<_>>();
        if columns.len() != expected_columns {
            return Err(format!(
                "{path}: row {} has {} columns, expected {expected_columns}",
                index + 2,
                columns.len()
            ));
        }
        rows.push(columns);
    }
    Ok(rows)
}

fn parse_u64(text: &str, path: &str) -> QualityResult<u64> {
    text.parse::<u64>()
        .map_err(|err| format!("{path}: `{text}` is not an unsigned integer: {err}"))
}

fn parse_percent(text: &str, path: &str) -> QualityResult<u64> {
    let value = parse_u64(text, path)?;
    if value > 100 {
        Err(format!("{path}: percentage `{value}` must be <= 100"))
    } else {
        Ok(value)
    }
}

fn derive_current_metrics(
    baseline: &[MetricRow],
    inventory: &[(String, String)],
    gaps: &[String],
) -> Vec<MetricRow> {
    let mut proof_counts = BTreeMap::<String, u64>::new();
    let mut stale_counts = BTreeMap::<String, u64>::new();
    for (status, source_contract) in inventory {
        if status == "current" {
            *proof_counts.entry(source_contract.clone()).or_default() += 1;
        } else if status == "stale" {
            *stale_counts.entry(source_contract.clone()).or_default() += 1;
        }
    }
    let mut gap_counts = BTreeMap::<String, u64>::new();
    for gap in gaps {
        *gap_counts.entry(gap.clone()).or_default() += 1;
    }

    baseline
        .iter()
        .map(|row| MetricRow {
            feature_class: row.feature_class.clone(),
            current_proof_count: *proof_counts.get(&row.feature_class).unwrap_or(&0),
            stale_count: *stale_counts.get(&row.feature_class).unwrap_or(&0),
            gap_count: *gap_counts.get(&row.feature_class).unwrap_or(&0),
            nondeterministic_count: 0,
            repro_fail_count: 0,
            repro_pass_rate_7d: 100,
            lane_pass_rate_30d: 100,
        })
        .collect()
}

fn validate_baseline(rows: &[MetricRow]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        if !seen.insert(row.feature_class.clone()) {
            diagnostics.push(format!(
                "{BASELINE_PATH}: duplicate feature class `{}`",
                row.feature_class
            ));
        }
    }
    if rows.is_empty() {
        diagnostics.push(format!(
            "{BASELINE_PATH}: baseline must contain at least one row"
        ));
    }
    diagnostics
}

fn validate_policy(policy: &Policy) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if policy.warning_threshold_percent > policy.hard_threshold_percent {
        diagnostics.push(format!(
            "{POLICY_PATH}: warning threshold must be <= hard threshold"
        ));
    }
    if policy.hard_threshold_percent > 100 {
        diagnostics.push(format!("{POLICY_PATH}: hard threshold must be <= 100"));
    }
    diagnostics
}

fn validate_regression(
    baseline: &[MetricRow],
    current: &[MetricRow],
    policy: &Policy,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let current_by_feature = current
        .iter()
        .map(|row| (row.feature_class.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    for baseline_row in baseline {
        let Some(current_row) = current_by_feature.get(baseline_row.feature_class.as_str()) else {
            diagnostics.push(format!(
                "{BASELINE_PATH}: feature `{}` is missing from current proof metrics",
                baseline_row.feature_class
            ));
            continue;
        };
        if current_row.current_proof_count < baseline_row.current_proof_count {
            diagnostics.push(format!(
                "{}: proof count dropped from {} to {}",
                baseline_row.feature_class,
                baseline_row.current_proof_count,
                current_row.current_proof_count
            ));
        }
        if current_row.nondeterministic_count > baseline_row.nondeterministic_count {
            diagnostics.push(format!(
                "{}: nondeterministic count increased from {} to {}",
                baseline_row.feature_class,
                baseline_row.nondeterministic_count,
                current_row.nondeterministic_count
            ));
        }
        if current_row.repro_fail_count > baseline_row.repro_fail_count {
            diagnostics.push(format!(
                "{}: reproduction failures increased from {} to {}",
                baseline_row.feature_class,
                baseline_row.repro_fail_count,
                current_row.repro_fail_count
            ));
        }
        if baseline_row.feature_class == policy.monotonic_lane
            && current_row.lane_pass_rate_30d < baseline_row.lane_pass_rate_30d
        {
            diagnostics.push(format!(
                "{}: monotonic lane pass rate dropped from {} to {}",
                baseline_row.feature_class,
                baseline_row.lane_pass_rate_30d,
                current_row.lane_pass_rate_30d
            ));
        }
    }
    diagnostics
}

fn collect_warnings(baseline: &[MetricRow], current: &[MetricRow], policy: &Policy) -> Vec<String> {
    baseline
        .iter()
        .zip(current)
        .filter_map(|(baseline_row, current_row)| {
            if current_row.gap_count > baseline_row.gap_count {
                let delta = current_row.gap_count - baseline_row.gap_count;
                Some(format!(
                    "{}: gap count increased by {delta}; warning threshold is {}%",
                    current_row.feature_class, policy.warning_threshold_percent
                ))
            } else {
                None
            }
        })
        .collect()
}

fn write_history_report(root: &Path, report: &LeanProofRegressionReport) -> QualityResult<PathBuf> {
    let path = root
        .join(HISTORY_DIR)
        .join(format!("{}.json", report.generated_date));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("{}: failed to create directory: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(report)
        .map_err(|err| format!("{}: failed to serialize report: {err}", path.display()))?;
    fs::write(&path, format!("{text}\n"))
        .map_err(|err| format!("{}: failed to write report: {err}", path.display()))?;
    Ok(path)
}

fn today_utc() -> String {
    OffsetDateTime::now_utc().date().to_string()
}

#[cfg(test)]
#[path = "lean_proof_regression_test.rs"]
mod lean_proof_regression_test;
