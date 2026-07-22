//! Versioned performance policy for comparable compilation reports.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::model::{CompilationBenchmarkReport, CompilationMeasurement};
use super::validate_report;

const POLICY_SCHEMA: &str = "terlan-aot-compilation-limits-v1";
const REQUIRED_SAMPLE_COUNT: usize = 7;
const MAXIMUM_COLD_RATIO_CEILING: f64 = 5.0;
const MAXIMUM_INCREMENTAL_RATIO_CEILING: f64 = 5.0;
const MAXIMUM_WARM_P95_NS_CEILING: u128 = 1_000_000_000;
const COLD_SCENARIOS: [&str; 3] = [
    "small_cold_development",
    "multi_cold_development",
    "cold_release",
];
const INCREMENTAL_SCENARIOS: [&str; 3] =
    ["one_package_edit", "no_op_development", "package_relink"];
const WARM_P95_SCENARIOS: [&str; 5] = [
    "one_package_edit",
    "no_op_development",
    "package_relink",
    "changed_repl",
    "unchanged_repl",
];

/// Deserialized ratio and latency limits for one release line.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompilationPerformancePolicy {
    /// Versioned policy schema.
    pub(super) schema: String,
    /// Exact samples required in every report row.
    pub(super) required_sample_count: usize,
    /// Canonical cold scenarios governed by ratio limits.
    pub(super) cold_scenarios: Vec<String>,
    /// Maximum permitted Terlan-to-Go cold median ratio.
    pub(super) maximum_cold_median_ratio: f64,
    /// Maximum permitted Terlan-to-Go cold p95 ratio.
    pub(super) maximum_cold_p95_ratio: f64,
    /// Canonical incremental scenarios governed by ratio limits.
    pub(super) incremental_scenarios: Vec<String>,
    /// Maximum permitted Terlan-to-Go incremental median ratio.
    pub(super) maximum_incremental_median_ratio: f64,
    /// Maximum permitted Terlan-to-Go incremental p95 ratio.
    pub(super) maximum_incremental_p95_ratio: f64,
    /// Warm scenarios governed by the absolute p95 latency limit.
    pub(super) warm_p95_scenarios: Vec<String>,
    /// Maximum permitted warm p95 latency in nanoseconds.
    pub(super) maximum_warm_p95_ns: u128,
}

/// Loads, structurally validates, and enforces one report and policy pair.
pub(super) fn validate_files(report_path: &Path, policy_path: &Path) -> Result<(), String> {
    let report_bytes = fs::read(report_path).map_err(|error| {
        format!(
            "failed to read compilation report `{}`: {error}",
            report_path.display()
        )
    })?;
    let report =
        serde_json::from_slice::<CompilationBenchmarkReport>(&report_bytes).map_err(|error| {
            format!(
                "failed to parse compilation report `{}`: {error}",
                report_path.display()
            )
        })?;
    let policy = load_policy(policy_path)?;
    validate_performance(&report, &policy)
}

/// Loads one strict policy document and rejects unknown fields.
pub(super) fn load_policy(path: &Path) -> Result<CompilationPerformancePolicy, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read compilation policy `{}`: {error}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to parse compilation policy `{}`: {error}",
            path.display()
        )
    })
}

/// Enforces canonical policy shape and every report ratio and warm latency.
pub(super) fn validate_performance(
    report: &CompilationBenchmarkReport,
    policy: &CompilationPerformancePolicy,
) -> Result<(), String> {
    validate_report(report)?;
    validate_policy(policy)?;
    if report.sample_count != policy.required_sample_count {
        return Err(format!(
            "error[aot.compilation.sample_count]: report has {} samples per row; policy requires {}",
            report.sample_count, policy.required_sample_count
        ));
    }
    validate_ratio_group(
        report,
        &policy.cold_scenarios,
        policy.maximum_cold_median_ratio,
        policy.maximum_cold_p95_ratio,
        "cold",
    )?;
    validate_ratio_group(
        report,
        &policy.incremental_scenarios,
        policy.maximum_incremental_median_ratio,
        policy.maximum_incremental_p95_ratio,
        "incremental",
    )?;
    for scenario in &policy.warm_p95_scenarios {
        let measurement = measurement(report, scenario)?;
        if measurement.terlan.p95_ns > policy.maximum_warm_p95_ns {
            return Err(format!(
                "error[aot.compilation.warm_p95]: `{scenario}` p95 {}ns exceeds {}ns",
                measurement.terlan.p95_ns, policy.maximum_warm_p95_ns
            ));
        }
    }
    Ok(())
}

/// Rejects policy drift, omitted scenarios, non-finite values, and weaker limits.
pub(super) fn validate_policy(policy: &CompilationPerformancePolicy) -> Result<(), String> {
    if policy.schema != POLICY_SCHEMA
        || policy.required_sample_count != REQUIRED_SAMPLE_COUNT
        || policy.cold_scenarios != COLD_SCENARIOS
        || policy.incremental_scenarios != INCREMENTAL_SCENARIOS
        || policy.warm_p95_scenarios != WARM_P95_SCENARIOS
    {
        return Err("error[aot.compilation.policy_shape]: compilation performance policy does not match the canonical v1 scenario contract".to_string());
    }
    validate_ratio_limit(
        "maximum_cold_median_ratio",
        policy.maximum_cold_median_ratio,
        MAXIMUM_COLD_RATIO_CEILING,
    )?;
    validate_ratio_limit(
        "maximum_cold_p95_ratio",
        policy.maximum_cold_p95_ratio,
        MAXIMUM_COLD_RATIO_CEILING,
    )?;
    validate_ratio_limit(
        "maximum_incremental_median_ratio",
        policy.maximum_incremental_median_ratio,
        MAXIMUM_INCREMENTAL_RATIO_CEILING,
    )?;
    validate_ratio_limit(
        "maximum_incremental_p95_ratio",
        policy.maximum_incremental_p95_ratio,
        MAXIMUM_INCREMENTAL_RATIO_CEILING,
    )?;
    if policy.maximum_warm_p95_ns == 0 || policy.maximum_warm_p95_ns > MAXIMUM_WARM_P95_NS_CEILING {
        return Err(format!(
            "error[aot.compilation.policy_weakened]: maximum_warm_p95_ns {} exceeds hard ceiling {}",
            policy.maximum_warm_p95_ns, MAXIMUM_WARM_P95_NS_CEILING
        ));
    }
    Ok(())
}

/// Enforces median and p95 Terlan-to-Go ratios for one scenario class.
fn validate_ratio_group(
    report: &CompilationBenchmarkReport,
    scenarios: &[String],
    maximum_median: f64,
    maximum_p95: f64,
    class: &str,
) -> Result<(), String> {
    for scenario in scenarios {
        let measurement = measurement(report, scenario)?;
        let median = measurement.median_ratio.ok_or_else(|| {
            format!("error[aot.compilation.ratio_missing]: `{scenario}` has no median ratio")
        })?;
        let p95 = measurement.p95_ratio.ok_or_else(|| {
            format!("error[aot.compilation.ratio_missing]: `{scenario}` has no p95 ratio")
        })?;
        if median > maximum_median {
            return Err(format!(
                "error[aot.compilation.{class}_median_ratio]: `{scenario}` ratio {median:.4} exceeds {maximum_median:.4}"
            ));
        }
        if p95 > maximum_p95 {
            return Err(format!(
                "error[aot.compilation.{class}_p95_ratio]: `{scenario}` ratio {p95:.4} exceeds {maximum_p95:.4}"
            ));
        }
    }
    Ok(())
}

/// Returns one required measurement by stable scenario identity.
fn measurement<'a>(
    report: &'a CompilationBenchmarkReport,
    scenario: &str,
) -> Result<&'a CompilationMeasurement, String> {
    report
        .measurements
        .iter()
        .find(|measurement| measurement.name == scenario)
        .ok_or_else(|| {
            format!("error[aot.compilation.measurement_missing]: `{scenario}` is absent")
        })
}

/// Rejects zero, non-finite, or policy-weakened ratio limits.
fn validate_ratio_limit(name: &str, value: f64, ceiling: f64) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 || value > ceiling {
        return Err(format!(
            "error[aot.compilation.policy_weakened]: {name} {value} exceeds hard ceiling {ceiling}"
        ));
    }
    Ok(())
}
