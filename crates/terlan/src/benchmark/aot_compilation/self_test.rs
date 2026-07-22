//! Production-compiled contract self-test for compilation benchmark reports.

use std::fs;
use std::path::Path;
use std::time::Duration;

use super::model::{
    CompilationBenchmarkReport, CompilationCacheState, CompilationFixtureIdentity,
    CompilationMeasurement, CompilationTiming, CompilationToolchains,
};
use super::policy::{load_policy, validate_performance, validate_policy};
use super::{
    copy_tree, create_workspace, fixture_sha256, replace_text, validate_report, REPORT_SCHEMA,
};
use crate::hardware::HardwareFingerprint;

/// Runs report, percentile, serialization, and fixture contract checks.
pub(super) fn run() -> Result<(), String> {
    timing_contract()?;
    report_contract()?;
    rejection_contract()?;
    policy_contract()?;
    fixture_contract()?;
    Ok(())
}

/// Proves timing summaries sort samples and compute nearest-rank percentiles.
fn timing_contract() -> Result<(), String> {
    let timing = timing()?;
    if timing.samples_ns != vec![1, 2, 3, 4, 5, 6, 7]
        || timing.min_ns != 1
        || timing.median_ns != 4
        || timing.p95_ns != 7
        || timing.max_ns != 7
    {
        return Err("compilation timing percentile contract changed".to_string());
    }
    Ok(())
}

/// Proves complete reports serialize, parse, and retain their measurement set.
fn report_contract() -> Result<(), String> {
    let report = report()?;
    validate_report(&report)?;
    let json = serde_json::to_vec(&report)
        .map_err(|error| format!("failed to serialize self-test report: {error}"))?;
    let parsed = serde_json::from_slice::<CompilationBenchmarkReport>(&json)
        .map_err(|error| format!("failed to parse self-test report: {error}"))?;
    validate_report(&parsed)?;
    if parsed.measurements.len() != 10 {
        return Err("compilation report lost measurements during round trip".to_string());
    }
    Ok(())
}

/// Proves incomplete measurements and synthetic Go REPL ratios are rejected.
fn rejection_contract() -> Result<(), String> {
    let mut missing = report()?;
    missing.measurements.pop();
    if validate_report(&missing).is_ok() {
        return Err("compilation report accepted a missing measurement".to_string());
    }
    let mut synthetic = report()?;
    synthetic.measurements[9].median_ratio = Some(1.0);
    if validate_report(&synthetic).is_ok() {
        return Err("compilation report accepted a synthetic Go REPL ratio".to_string());
    }
    let mut forged = report()?;
    forged.measurements[0].median_ratio = Some(0.5);
    if validate_report(&forged).is_ok() {
        return Err("compilation report accepted a ratio inconsistent with timing".to_string());
    }
    Ok(())
}

/// Proves policy weakening, over-budget ratios, and warm regressions fail.
fn policy_contract() -> Result<(), String> {
    let policy_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("benchmarks/baselines/aot-compilation-limits.json");
    let policy = load_policy(&policy_path)?;
    let baseline = report()?;
    validate_performance(&baseline, &policy)?;

    let workspace = create_workspace()?;
    let unknown_field_path = workspace.join("unknown-policy.json");
    let policy_json = fs::read_to_string(&policy_path)
        .map_err(|error| format!("failed to read compilation policy fixture: {error}"))?;
    fs::write(
        &unknown_field_path,
        policy_json.replacen('{', "{\n  \"unexpected\": true,", 1),
    )
    .map_err(|error| format!("failed to write invalid compilation policy: {error}"))?;
    let unknown_field_result = load_policy(&unknown_field_path);
    let _ = fs::remove_dir_all(workspace);
    if unknown_field_result.is_ok() {
        return Err("compilation policy accepted an unknown field".to_string());
    }

    let mut weakened = policy.clone();
    weakened.maximum_cold_p95_ratio = 5.1;
    if validate_policy(&weakened).is_ok() {
        return Err("compilation policy accepted a weakened cold ratio".to_string());
    }

    let mut weakened_incremental = policy.clone();
    weakened_incremental.maximum_incremental_median_ratio = 5.1;
    if validate_policy(&weakened_incremental).is_ok() {
        return Err("compilation policy accepted a weakened incremental ratio".to_string());
    }

    let mut incomplete = policy.clone();
    incomplete.incremental_scenarios.pop();
    if validate_policy(&incomplete).is_ok() {
        return Err("compilation policy accepted an incomplete scenario set".to_string());
    }

    let mut slow_cold = report()?;
    slow_cold.measurements[0].terlan = timing_at(6_000)?;
    slow_cold.measurements[0].go = Some(timing_at(1_000)?);
    slow_cold.measurements[0].median_ratio = Some(6.0);
    slow_cold.measurements[0].p95_ratio = Some(6.0);
    if validate_performance(&slow_cold, &policy).is_ok() {
        return Err("compilation policy accepted an over-budget cold ratio".to_string());
    }

    let mut slow_warm = report()?;
    slow_warm.measurements[8].terlan = timing_at(1_000_000_001)?;
    if validate_performance(&slow_warm, &policy).is_ok() {
        return Err("compilation policy accepted warm p95 above one second".to_string());
    }
    Ok(())
}

/// Proves committed fixtures hash deterministically and retain exact edit points.
fn fixture_contract() -> Result<(), String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("benchmarks/fixtures/aot_compilation");
    let first = fixture_sha256(&fixtures)?;
    let second = fixture_sha256(&fixtures)?;
    if first != second || first.len() != 64 {
        return Err("compilation fixture digest is not deterministic".to_string());
    }
    let copy = create_workspace()?;
    let result = (|| {
        copy_tree(&fixtures, &copy)?;
        let dependency = copy.join("terlan/multi/src/aotbench/Math.terl");
        replace_text(&dependency, "    7.", "    8.")?;
        let edited = fs::read_to_string(&dependency)
            .map_err(|error| format!("failed to read edited fixture: {error}"))?;
        if !edited.contains("    8.") {
            return Err("compilation fixture edit did not persist".to_string());
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(copy);
    result
}

/// Builds one valid synthetic timing summary with seven sorted samples.
fn timing() -> Result<CompilationTiming, String> {
    CompilationTiming::from_durations(
        [7, 1, 5, 3, 2, 6, 4]
            .into_iter()
            .map(Duration::from_nanos)
            .collect(),
    )
}

/// Builds one seven-sample timing summary with a fixed nanosecond value.
fn timing_at(nanoseconds: u64) -> Result<CompilationTiming, String> {
    CompilationTiming::from_durations(vec![Duration::from_nanos(nanoseconds); 7])
}

/// Builds one valid complete report without running host compilers.
fn report() -> Result<CompilationBenchmarkReport, String> {
    let names = [
        "small_cold_development",
        "multi_cold_development",
        "one_package_edit",
        "no_op_development",
        "cold_release",
        "package_relink",
        "repl_startup",
        "first_repl",
        "changed_repl",
        "unchanged_repl",
    ];
    let mut measurements = Vec::with_capacity(names.len());
    for (index, name) in names.into_iter().enumerate() {
        let comparable = index < 6;
        measurements.push(CompilationMeasurement {
            name: name.to_string(),
            scope: "test scope".to_string(),
            terlan: timing()?,
            go: if comparable { Some(timing()?) } else { None },
            median_ratio: comparable.then_some(1.0),
            p95_ratio: comparable.then_some(1.0),
            reference_note: (!comparable).then(|| "Go has no REPL".to_string()),
        });
    }
    Ok(CompilationBenchmarkReport {
        schema: REPORT_SCHEMA.to_string(),
        status: "completed".to_string(),
        recorded_unix_seconds: 1,
        hardware: HardwareFingerprint {
            schema: "terlan-benchmark-hardware-v1".to_string(),
            operating_system: "linux".to_string(),
            architecture: "x86_64".to_string(),
            cpu_model: "test-cpu".to_string(),
            logical_cpu_count: 8,
            rustc_version: "rustc test".to_string(),
            sha256: "a".repeat(64),
        },
        toolchains: CompilationToolchains {
            rustc: "rustc test".to_string(),
            go: "go test".to_string(),
            terlc_path: "/test/terlc".to_string(),
            terlc_sha256: "b".repeat(64),
        },
        fixtures: CompilationFixtureIdentity {
            path: "benchmarks/fixtures/aot_compilation".to_string(),
            sha256: "c".repeat(64),
            workloads: vec!["small-command".to_string(), "multi-package".to_string()],
        },
        sample_count: 7,
        cache_state: CompilationCacheState {
            terlan_cold: "fresh".to_string(),
            go_cold: "fresh command".to_string(),
            warm: "populated".to_string(),
            dependency_downloads_timed: false,
        },
        measurements,
    })
}
