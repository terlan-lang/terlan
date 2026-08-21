use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::terlan_quality::QualityResult;

const PROFILE_PATH: &str = "benches/http/PROFILE.toml";
const COMPARABILITY_REPORT_PATH: &str =
    "target/quality/vm-http-benchmark-comparability-report.json";
const ATTRIBUTION_REPORT_PATH: &str =
    "target/quality/vm-http-runtime-attribution-contract-report.json";

const REQUIRED_STACKS: &[&str] = &["terlan-vm", "axum", "hyper"];
const REQUIRED_METRICS: &[&str] = &[
    "mean_us",
    "p50_us",
    "p95_us",
    "p99_us",
    "throughput_requests_per_second",
];
const REQUIRED_CONCURRENCY: &[u32] = &[1, 10, 100, 1000];
const REQUIRED_PAYLOADS: &[u32] = &[0, 512, 4096];
const REQUIRED_ROUTES: &[&str] = &["static", "json", "add", "route-param", "stateful-counter"];
const REQUIRED_SCENARIOS: &[&str] = &[
    "malformed-headers",
    "large-headers",
    "slow-client",
    "cancellation",
    "backpressure",
];
const REQUIRED_ATTRIBUTION_BUCKETS: &[&str] = &[
    "transportNs",
    "parserNs",
    "schedulerNs",
    "routingNs",
    "allocationAndConversionNs",
    "handlerNs",
    "responseWriteNs",
];
const REQUIRED_ATTRIBUTION_INVARIANTS: &[&str] = &[
    "completedMatchesReductions",
    "phaseBucketsMatchAccountedTotal",
    "queueBalanced",
    "parkedProcessesReleased",
    "saturationHasBackpressureOutcome",
];
const REQUIRED_AOT_REPLAY_INTEGRATION: &[&str] = &[
    "AotHandlerGeneration",
    "multicore_replay_evidence",
    "multicore_replay_capture",
    "VmMulticoreReplayEvidence",
];
const REQUIRED_AOT_REPLAY_EVIDENCE: &[&str] = &[
    "terlan.vm.multicore-replay.v1",
    "VmMulticoreReplayEvidence",
    "retained_events",
    "dropped_events",
    "replayable",
];

#[derive(Debug, Deserialize)]
struct BenchmarkProfile {
    schema: String,
    sample_count: u32,
    regression_threshold_percent: u32,
    stacks: Vec<String>,
    metrics: Vec<String>,
    schedule: BenchmarkSchedule,
    replay: ReplayContract,
    adversarial: AdversarialContract,
}

#[derive(Debug, Deserialize)]
struct BenchmarkSchedule {
    fixed_total_requests: u32,
    warmup_requests: u32,
    protocol: String,
    tls_mode: String,
    parser_mode: String,
    keep_alive_policy: String,
    concurrency: Vec<u32>,
    payload_bytes: Vec<u32>,
    route_mix: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReplayContract {
    fingerprint_schema: String,
    execution_validation_required: bool,
    stable_runs_required: u32,
}

#[derive(Debug, Deserialize)]
struct AdversarialContract {
    scenarios: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm http benchmark comparability summary.
pub struct VmHttpBenchmarkComparabilitySummary {
    pub profile_fingerprint: String,
    pub concurrency_count: usize,
    pub scenario_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm http runtime attribution contract summary.
pub struct VmHttpRuntimeAttributionContractSummary {
    pub bucket_count: usize,
    pub invariant_count: usize,
    pub report_path: PathBuf,
}

/// Validates the product-owned benchmark schedule shared by external HTTP lanes.
pub fn run_vm_http_benchmark_comparability(
    root: &Path,
) -> QualityResult<VmHttpBenchmarkComparabilitySummary> {
    let (profile, profile_text) = read_profile(root)?;
    let diagnostics = validate_profile(&profile);
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-http-benchmark-comparability",
            &diagnostics,
        ));
    }

    let fingerprint = Sha256::digest(profile_text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let report = json!({
        "schema": "terlan-vm-http-benchmark-comparability-contract-v1",
        "profilePath": PROFILE_PATH,
        "profileFingerprintSha256": fingerprint,
        "sampleCount": profile.sample_count,
        "regressionThresholdPercent": profile.regression_threshold_percent,
        "fixedTotalRequests": profile.schedule.fixed_total_requests,
        "warmupRequests": profile.schedule.warmup_requests,
        "stacks": profile.stacks,
        "metrics": profile.metrics,
        "concurrency": profile.schedule.concurrency,
        "payloadBytes": profile.schedule.payload_bytes,
        "routeMix": profile.schedule.route_mix,
        "adversarialScenarios": profile.adversarial.scenarios,
        "replayFingerprintSchema": profile.replay.fingerprint_schema,
        "executionValidationRequired": profile.replay.execution_validation_required
    });
    let report_path = write_report(root, COMPARABILITY_REPORT_PATH, &report)?;

    Ok(VmHttpBenchmarkComparabilitySummary {
        profile_fingerprint: fingerprint,
        concurrency_count: profile.schedule.concurrency.len(),
        scenario_count: profile.adversarial.scenarios.len(),
        report_path,
    })
}

/// Validates attribution telemetry and canonical check/release ownership.
pub fn run_vm_http_runtime_attribution_contract(
    root: &Path,
) -> QualityResult<VmHttpRuntimeAttributionContractSummary> {
    let (profile, _) = read_profile(root)?;
    let mut diagnostics = validate_profile(&profile);
    diagnostics.extend(validate_source_terms(
        root,
        "crates/terlan/src/vm/main/http_attribution.rs",
        REQUIRED_ATTRIBUTION_BUCKETS,
        "attribution bucket",
    )?);
    diagnostics.extend(validate_source_terms(
        root,
        "crates/terlan/src/vm/main/http_attribution.rs",
        REQUIRED_ATTRIBUTION_INVARIANTS,
        "attribution invariant",
    )?);
    diagnostics.extend(validate_source_terms(
        root,
        "crates/terlan/src/commands/serve/handler_cache/replay_evidence.rs",
        REQUIRED_AOT_REPLAY_INTEGRATION,
        "AOT replay integration",
    )?);
    diagnostics.extend(validate_source_terms(
        root,
        "crates/terlan/src/runtime/vm/multicore_replay.rs",
        REQUIRED_AOT_REPLAY_EVIDENCE,
        "AOT replay evidence",
    )?);
    diagnostics.extend(validate_make_ownership(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-http-runtime-attribution", &diagnostics));
    }

    let report = json!({
        "schema": "terlan-vm-http-runtime-attribution-contract-v1",
        "runtimeSchema": "terlan-vm-http-runtime-attribution-v1",
        "externalEvidenceSchema": "terlan-vm-http-runtime-attribution-comparison-v1",
        "externalEvidenceOwnership": "workspace-benchmarks-outside-golden-release",
        "requiredBuckets": REQUIRED_ATTRIBUTION_BUCKETS,
        "requiredInvariants": REQUIRED_ATTRIBUTION_INVARIANTS,
        "dominantCauseRequired": true,
        "sourceCounterRequired": true,
        "boundedAotReplayRequired": true,
        "checkOrder": [
            "vm-http-benchmark-comparability-check",
            "vm-http-runtime-attribution-check"
        ],
        "ownedByCheck": true,
        "ownedByReleasePreflight": true
    });
    let report_path = write_report(root, ATTRIBUTION_REPORT_PATH, &report)?;
    Ok(VmHttpRuntimeAttributionContractSummary {
        bucket_count: REQUIRED_ATTRIBUTION_BUCKETS.len(),
        invariant_count: REQUIRED_ATTRIBUTION_INVARIANTS.len(),
        report_path,
    })
}

fn read_profile(root: &Path) -> QualityResult<(BenchmarkProfile, String)> {
    let path = root.join(PROFILE_PATH);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read benchmark profile: {err}",
            path.display()
        )
    })?;
    let profile = basic_toml::from_str(&text)
        .map_err(|err| format!("{}: invalid benchmark profile: {err}", path.display()))?;
    Ok((profile, text))
}

fn validate_profile(profile: &BenchmarkProfile) -> Vec<String> {
    let mut diagnostics = Vec::new();
    require_equal(
        &mut diagnostics,
        "schema",
        &profile.schema,
        "terlan-vm-http-comparability-profile-v1",
    );
    if profile.sample_count < 3 || profile.replay.stable_runs_required < 3 {
        diagnostics.push("benchmark profile must require at least three stable runs".to_string());
    }
    if profile.regression_threshold_percent > 15 {
        diagnostics.push("benchmark regression threshold must not exceed 15 percent".to_string());
    }
    if profile.schedule.fixed_total_requests == 0 || profile.schedule.warmup_requests == 0 {
        diagnostics
            .push("benchmark schedule must define fixed load and warmup requests".to_string());
    }
    require_equal(
        &mut diagnostics,
        "protocol",
        &profile.schedule.protocol,
        "http1",
    );
    require_equal(
        &mut diagnostics,
        "TLS mode",
        &profile.schedule.tls_mode,
        "disabled-for-all-stacks",
    );
    require_equal(
        &mut diagnostics,
        "parser mode",
        &profile.schedule.parser_mode,
        "full-stack-parser",
    );
    require_equal(
        &mut diagnostics,
        "keep-alive policy",
        &profile.schedule.keep_alive_policy,
        "matched-per-lane",
    );
    if profile.replay.fingerprint_schema != "terlan.vm.multicore-replay.v1"
        || !profile.replay.execution_validation_required
    {
        diagnostics.push("benchmark replay must require validated v1 fingerprints".to_string());
    }
    require_strings(&mut diagnostics, "stack", &profile.stacks, REQUIRED_STACKS);
    require_strings(
        &mut diagnostics,
        "metric",
        &profile.metrics,
        REQUIRED_METRICS,
    );
    require_numbers(
        &mut diagnostics,
        "concurrency",
        &profile.schedule.concurrency,
        REQUIRED_CONCURRENCY,
    );
    require_numbers(
        &mut diagnostics,
        "payload",
        &profile.schedule.payload_bytes,
        REQUIRED_PAYLOADS,
    );
    require_strings(
        &mut diagnostics,
        "route",
        &profile.schedule.route_mix,
        REQUIRED_ROUTES,
    );
    require_strings(
        &mut diagnostics,
        "adversarial scenario",
        &profile.adversarial.scenarios,
        REQUIRED_SCENARIOS,
    );
    diagnostics
}

fn require_equal(diagnostics: &mut Vec<String>, label: &str, actual: &str, expected: &str) {
    if actual != expected {
        diagnostics.push(format!(
            "benchmark {label} must be `{expected}`, found `{actual}`"
        ));
    }
}

fn require_strings(
    diagnostics: &mut Vec<String>,
    label: &str,
    actual: &[String],
    required: &[&str],
) {
    for value in required {
        if !actual.iter().any(|actual| actual == value) {
            diagnostics.push(format!("benchmark profile is missing {label} `{value}`"));
        }
    }
}

fn require_numbers(diagnostics: &mut Vec<String>, label: &str, actual: &[u32], required: &[u32]) {
    for value in required {
        if !actual.contains(value) {
            diagnostics.push(format!("benchmark profile is missing {label} `{value}`"));
        }
    }
}

fn validate_source_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read {label} source: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} `{term}`"))
        .collect())
}

fn validate_make_ownership(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read attribution ownership: {err}"))?;
    let required = [
        "VM_HTTP_BENCHMARK_COMPARABILITY_DEPS := vm-http-concurrency-investigation-check",
        "vm-http-benchmark-comparability-check: $(VM_HTTP_BENCHMARK_COMPARABILITY_DEPS)",
        "vm-http-runtime-attribution-check: vm-http-benchmark-comparability-check",
        "vm-http-runtime-attribution-check \\",
        "vm-http-vs-axum-check: tvm-http-paired-performance-check\n",
        "RELEASE_EVIDENCE_GATES := \\\n\tvm-http-runtime-attribution-check \\",
    ];
    Ok(required
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing canonical attribution ownership `{term}`"))
        .collect())
}

fn write_report(root: &Path, relative: &str, report: &serde_json::Value) -> QualityResult<PathBuf> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(report)
        .map_err(|err| format!("{relative}: failed to serialize report: {err}"))?;
    fs::write(&path, format!("{text}\n"))
        .map_err(|err| format!("{relative}: failed to write report: {err}"))?;
    Ok(path)
}

fn render_failure(label: &str, diagnostics: &[String]) -> String {
    format!("[{label}] failures:\n  - {}", diagnostics.join("\n  - "))
}

#[cfg(test)]
#[path = "vm_http_benchmark_contract_test.rs"]
#[cfg(test)]
mod vm_http_benchmark_contract_test;
