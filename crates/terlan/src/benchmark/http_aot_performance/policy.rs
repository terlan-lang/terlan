//! Versioned HTTP AOT performance regression policy.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::HttpPerformanceReport;

const POLICY_SCHEMA: &str = "terlan-http-aot-performance-limits-v1";
const MAXIMUM_LATENCY_RATIO_CEILING: f64 = 1.50;
const MINIMUM_THROUGHPUT_RATIO_FLOOR: f64 = 0.70;
const MAXIMUM_PEAK_RSS_RATIO_CEILING: f64 = 1.20;
const MAXIMUM_RELOAD_RATIO_CEILING: f64 = 1.25;
const CANONICAL_POLICY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../benchmarks/baselines/http-aot-performance-limits.json"
));

/// Reviewed quantitative limits for the native-AOT HTTP lane.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HttpPerformancePolicy {
    /// Versioned policy schema.
    pub(super) schema: String,
    /// Maximum native/reference sequential p50 latency ratio.
    pub(super) maximum_sequential_p50_ratio: f64,
    /// Maximum native/reference sequential p95 latency ratio.
    pub(super) maximum_sequential_p95_ratio: f64,
    /// Maximum native/reference sequential p99 latency ratio.
    pub(super) maximum_sequential_p99_ratio: f64,
    /// Minimum native/reference sequential throughput ratio.
    pub(super) minimum_sequential_throughput_ratio: f64,
    /// Maximum native/reference pressure p50 latency ratio.
    pub(super) maximum_pressure_p50_ratio: f64,
    /// Maximum native/reference pressure p95 latency ratio.
    pub(super) maximum_pressure_p95_ratio: f64,
    /// Maximum native/reference pressure p99 latency ratio.
    pub(super) maximum_pressure_p99_ratio: f64,
    /// Minimum native/reference pressure throughput ratio.
    pub(super) minimum_pressure_throughput_ratio: f64,
    /// Maximum native/reference longevity p50 latency ratio.
    pub(super) maximum_longevity_p50_ratio: f64,
    /// Maximum native/reference longevity p95 latency ratio.
    pub(super) maximum_longevity_p95_ratio: f64,
    /// Maximum native/reference longevity p99 latency ratio.
    pub(super) maximum_longevity_p99_ratio: f64,
    /// Minimum native/reference longevity throughput ratio.
    pub(super) minimum_longevity_throughput_ratio: f64,
    /// Maximum native/reference peak resident-memory ratio.
    pub(super) maximum_peak_rss_ratio: f64,
    /// Maximum native/reference generation reload latency ratio.
    pub(super) maximum_generation_reload_ratio: f64,
}

/// Parses a strict policy document and rejects a weak or malformed budget.
pub(super) fn parse_policy(path: &Path, bytes: &[u8]) -> Result<HttpPerformancePolicy, String> {
    let policy = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "failed to parse HTTP AOT performance policy `{}`: {error}",
            path.display()
        )
    })?;
    validate_policy(&policy)?;
    Ok(policy)
}

/// Loads the compile-time canonical policy for pure comparison tests.
pub(super) fn canonical_policy() -> Result<HttpPerformancePolicy, String> {
    parse_policy(
        Path::new("embedded canonical HTTP policy"),
        CANONICAL_POLICY.as_bytes(),
    )
}

/// Enforces every quantitative native/reference budget.
pub(super) fn validate_performance(
    checked: &HttpPerformanceReport,
    native: &HttpPerformanceReport,
    policy: &HttpPerformancePolicy,
) -> Result<(), String> {
    validate_policy(policy)?;
    require_maximum(
        "sequential_p50",
        ratio(native.sequential.p50_ns, checked.sequential.p50_ns)?,
        policy.maximum_sequential_p50_ratio,
    )?;
    require_maximum(
        "sequential_p95",
        ratio(native.sequential.p95_ns, checked.sequential.p95_ns)?,
        policy.maximum_sequential_p95_ratio,
    )?;
    require_maximum(
        "sequential_p99",
        ratio(native.sequential.p99_ns, checked.sequential.p99_ns)?,
        policy.maximum_sequential_p99_ratio,
    )?;
    require_minimum(
        "sequential_throughput",
        ratio(
            native.sequential.throughput_requests_per_second,
            checked.sequential.throughput_requests_per_second,
        )?,
        policy.minimum_sequential_throughput_ratio,
    )?;
    require_maximum(
        "pressure_p50",
        ratio(
            native.pressure.timing.p50_ns,
            checked.pressure.timing.p50_ns,
        )?,
        policy.maximum_pressure_p50_ratio,
    )?;
    require_maximum(
        "pressure_p95",
        ratio(
            native.pressure.timing.p95_ns,
            checked.pressure.timing.p95_ns,
        )?,
        policy.maximum_pressure_p95_ratio,
    )?;
    require_maximum(
        "pressure_p99",
        ratio(
            native.pressure.timing.p99_ns,
            checked.pressure.timing.p99_ns,
        )?,
        policy.maximum_pressure_p99_ratio,
    )?;
    require_minimum(
        "pressure_throughput",
        ratio(
            native.pressure.timing.throughput_requests_per_second,
            checked.pressure.timing.throughput_requests_per_second,
        )?,
        policy.minimum_pressure_throughput_ratio,
    )?;
    require_maximum(
        "longevity_p50",
        ratio(
            native.longevity.timing.p50_ns,
            checked.longevity.timing.p50_ns,
        )?,
        policy.maximum_longevity_p50_ratio,
    )?;
    require_maximum(
        "longevity_p95",
        ratio(
            native.longevity.timing.p95_ns,
            checked.longevity.timing.p95_ns,
        )?,
        policy.maximum_longevity_p95_ratio,
    )?;
    require_maximum(
        "longevity_p99",
        ratio(
            native.longevity.timing.p99_ns,
            checked.longevity.timing.p99_ns,
        )?,
        policy.maximum_longevity_p99_ratio,
    )?;
    require_minimum(
        "longevity_throughput",
        ratio(
            native.longevity.timing.throughput_requests_per_second,
            checked.longevity.timing.throughput_requests_per_second,
        )?,
        policy.minimum_longevity_throughput_ratio,
    )?;
    require_maximum(
        "peak_rss",
        ratio(
            u128::from(required_peak_rss(native)?),
            u128::from(required_peak_rss(checked)?),
        )?,
        policy.maximum_peak_rss_ratio,
    )?;
    require_maximum(
        "generation_reload",
        ratio(
            native.generation_overlap.reload_latency_ns,
            checked.generation_overlap.reload_latency_ns,
        )?,
        policy.maximum_generation_reload_ratio,
    )
}

/// Rejects policy drift and any budget weaker than the hard v1 ceilings.
pub(super) fn validate_policy(policy: &HttpPerformancePolicy) -> Result<(), String> {
    if policy.schema != POLICY_SCHEMA {
        return Err(
            "error[aot.http.performance.policy_shape]: unexpected HTTP performance policy schema"
                .to_string(),
        );
    }
    validate_maximum(
        "maximum_sequential_p50_ratio",
        policy.maximum_sequential_p50_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_maximum(
        "maximum_sequential_p95_ratio",
        policy.maximum_sequential_p95_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_maximum(
        "maximum_sequential_p99_ratio",
        policy.maximum_sequential_p99_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_minimum(
        "minimum_sequential_throughput_ratio",
        policy.minimum_sequential_throughput_ratio,
        MINIMUM_THROUGHPUT_RATIO_FLOOR,
    )?;
    validate_maximum(
        "maximum_pressure_p50_ratio",
        policy.maximum_pressure_p50_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_maximum(
        "maximum_pressure_p95_ratio",
        policy.maximum_pressure_p95_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_maximum(
        "maximum_pressure_p99_ratio",
        policy.maximum_pressure_p99_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_minimum(
        "minimum_pressure_throughput_ratio",
        policy.minimum_pressure_throughput_ratio,
        MINIMUM_THROUGHPUT_RATIO_FLOOR,
    )?;
    validate_maximum(
        "maximum_longevity_p50_ratio",
        policy.maximum_longevity_p50_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_maximum(
        "maximum_longevity_p95_ratio",
        policy.maximum_longevity_p95_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_maximum(
        "maximum_longevity_p99_ratio",
        policy.maximum_longevity_p99_ratio,
        MAXIMUM_LATENCY_RATIO_CEILING,
    )?;
    validate_minimum(
        "minimum_longevity_throughput_ratio",
        policy.minimum_longevity_throughput_ratio,
        MINIMUM_THROUGHPUT_RATIO_FLOOR,
    )?;
    validate_maximum(
        "maximum_peak_rss_ratio",
        policy.maximum_peak_rss_ratio,
        MAXIMUM_PEAK_RSS_RATIO_CEILING,
    )?;
    validate_maximum(
        "maximum_generation_reload_ratio",
        policy.maximum_generation_reload_ratio,
        MAXIMUM_RELOAD_RATIO_CEILING,
    )
}

fn required_peak_rss(report: &HttpPerformanceReport) -> Result<u64, String> {
    report
        .allocation
        .peak_observed_bytes
        .filter(|value| *value > 0)
        .ok_or_else(|| "error[aot.http.performance.peak_rss]: missing peak RSS".to_string())
}

fn ratio(numerator: u128, denominator: u128) -> Result<f64, String> {
    if numerator == 0 || denominator == 0 {
        return Err("error[aot.http.performance.ratio]: performance ratio input is zero".into());
    }
    Ok(numerator as f64 / denominator as f64)
}

fn require_maximum(name: &str, observed: f64, maximum: f64) -> Result<(), String> {
    if observed > maximum {
        return Err(format!(
            "error[aot.http.performance.{name}]: native/reference ratio {observed:.4} exceeds reviewed maximum {maximum:.4}"
        ));
    }
    Ok(())
}

fn require_minimum(name: &str, observed: f64, minimum: f64) -> Result<(), String> {
    if observed < minimum {
        return Err(format!(
            "error[aot.http.performance.{name}]: native/reference ratio {observed:.4} is below reviewed minimum {minimum:.4}"
        ));
    }
    Ok(())
}

fn validate_maximum(name: &str, value: f64, ceiling: f64) -> Result<(), String> {
    if !value.is_finite() || value <= 0.0 || value > ceiling {
        return Err(format!(
            "error[aot.http.performance.policy_weakened]: {name} {value} exceeds hard ceiling {ceiling}"
        ));
    }
    Ok(())
}

fn validate_minimum(name: &str, value: f64, floor: f64) -> Result<(), String> {
    if !value.is_finite() || value > 1.0 || value < floor {
        return Err(format!(
            "error[aot.http.performance.policy_weakened]: {name} {value} is below hard floor {floor}"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
