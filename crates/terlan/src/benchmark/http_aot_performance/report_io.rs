//! Report file and environment helpers for the HTTP AOT benchmark.

use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::{
    sha256, HttpExecutionLane, HttpMeasurementEvidence, HttpPerformanceReport,
    LEGACY_CHECKED_COREIR_SCHEMA,
};

/// Computes the digest of one compiler or report file.
pub(super) fn sha256_file(path: &Path) -> Result<String, String> {
    read_file(path).map(|bytes| sha256(&bytes))
}

/// Reads one required file with path-aware diagnostics.
pub(super) fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("failed to read `{}`: {error}", path.display()))
}

/// Parses one typed lane report.
pub(super) fn parse_report(path: &Path, bytes: &[u8]) -> Result<HttpPerformanceReport, String> {
    let mut report: HttpPerformanceReport = serde_json::from_slice(bytes)
        .map_err(|error| format!("failed to parse `{}`: {error}", path.display()))?;
    if report.schema == LEGACY_CHECKED_COREIR_SCHEMA {
        adapt_legacy_checked_coreir(path, &mut report)?;
    }
    Ok(report)
}

/// Adapts the irreplaceable v1 checked-CoreIR capture without fabricating data.
fn adapt_legacy_checked_coreir(
    path: &Path,
    report: &mut HttpPerformanceReport,
) -> Result<(), String> {
    if report.lane != HttpExecutionLane::CheckedCoreir
        || report.workload.warmup_requests != 0
        || report.workload.measurement_rounds != 0
        || report.workload.readiness_reactors != 0
        || !report.measurement.sequential_rounds.is_empty()
        || !report.measurement.pressure_rounds.is_empty()
        || !report.measurement.longevity_rounds.is_empty()
    {
        return Err(format!(
            "legacy checked-CoreIR report `{}` has unexpected v2 fields",
            path.display()
        ));
    }
    report.workload.measurement_rounds = 1;
    report.measurement = HttpMeasurementEvidence {
        aggregation: "median-throughput-round".to_string(),
        sequential_rounds: vec![report.sequential.clone()],
        pressure_rounds: vec![report.pressure.timing.clone()],
        longevity_rounds: vec![report.longevity.timing.clone()],
    };
    Ok(())
}

/// Exercises the production v1 adapter from the executable self-test.
pub(super) fn self_test_legacy_adapter() -> Result<(), String> {
    let mut value = serde_json::to_value(super::fixture_report(HttpExecutionLane::CheckedCoreir))
        .map_err(|error| format!("failed to encode legacy adapter fixture: {error}"))?;
    value["schema"] = serde_json::json!(LEGACY_CHECKED_COREIR_SCHEMA);
    value
        .as_object_mut()
        .ok_or_else(|| "legacy adapter fixture is not an object".to_string())?
        .remove("measurement");
    let workload = value["workload"]
        .as_object_mut()
        .ok_or_else(|| "legacy adapter workload is not an object".to_string())?;
    for field in [
        "warmup_requests",
        "measurement_rounds",
        "readiness_reactors",
    ] {
        workload.remove(field);
    }
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| format!("failed to encode legacy adapter JSON: {error}"))?;
    let report = parse_report(Path::new("checked-coreir-v1.json"), &bytes)?;
    if report.workload.measurement_rounds != 1
        || report.measurement.sequential_rounds.len() != 1
        || report.measurement.sequential_rounds[0].p50_ns != report.sequential.p50_ns
    {
        return Err("legacy checked-CoreIR adapter changed recorded measurements".to_string());
    }
    Ok(())
}

/// Writes one pretty JSON report after creating its parent directory.
pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create `{}`: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize HTTP benchmark report: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("failed to write `{}`: {error}", path.display()))
}

/// Reads a positive integer environment option or returns its default.
pub(super) fn read_positive_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(super) fn read_nonnegative_env(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

/// Returns the current Unix timestamp in seconds.
pub(super) fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Returns the current Unix timestamp in nanoseconds for unique paths.
pub(super) fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}
