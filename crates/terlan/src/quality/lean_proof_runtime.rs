use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::terlan_quality::{render_failure, QualityResult};

const RUNNER_CONFIG_PATH: &str = "proofs/lean/ci/lean-proof-runner.toml";
const REPORT_PATH: &str = "target/quality/proof-artifacts/lean-proof-runtime-policy.json";

const REQUIRED_GROUPS: &[&str] = &["foundational", "lowering", "runtime", "std-boundary"];

/// Summary produced by the Lean proof runtime-profile gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanProofRuntimeSummary {
    pub group_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LeanProofRunnerConfig {
    runner: RunnerConfig,
    guardrails: GuardrailConfig,
    groups: Vec<GroupConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RunnerConfig {
    lockstep_mode: bool,
    temp_root: String,
    clean_env: bool,
    forbidden_env: Vec<String>,
    lean_version: String,
    elan_channel: String,
    lake_flags: Vec<String>,
    dependency_lockfile: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GuardrailConfig {
    warning_wall_time_multiplier: u64,
    hard_wall_time_multiplier: u64,
    closeout_requires_lockstep: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GroupConfig {
    name: String,
    max_parallelism: u64,
    timeout_ms: u64,
    cpu_ms: u64,
    memory_mb: u64,
    io_mb: u64,
    retry_count: u64,
    lockstep: bool,
}

#[derive(Debug, Serialize)]
struct LeanProofRuntimeReport {
    schema: &'static str,
    config_path: &'static str,
    configured_policy: LeanProofRunnerConfig,
}

/// Runs Lean proof runtime-profile validation.
///
/// Inputs:
/// - `root`: repository root containing the Lean proof runner TOML.
///
/// Output:
/// - Success summary with configured scheduling-group count and report path.
/// - Stable diagnostics for missing groups, unsafe environment sharing,
///   malformed budgets, or non-deterministic closeout settings.
///
/// Transformation:
/// - Validates configured limits without executing proofs or claiming measured
///   resource usage, and writes a policy report separate from proof verdicts.
pub fn run_lean_proof_runtime(root: &Path) -> QualityResult<LeanProofRuntimeSummary> {
    let path =
        super::lean_proof_track::outputs::single(root, REPORT_PATH, "TERLAN_PROOF_POLICY_OUTPUT")?;
    run_to(root, &path)
}

fn run_to(root: &Path, path: &Path) -> QualityResult<LeanProofRuntimeSummary> {
    let config = parse_config(&read_text(root, RUNNER_CONFIG_PATH)?)?;
    let diagnostics = validate_config(&config);
    if !diagnostics.is_empty() {
        return Err(render_failure("lean-proof-runtime", &diagnostics));
    }

    let report_path = write_report(path, &runtime_report(&config))?;
    Ok(LeanProofRuntimeSummary {
        group_count: config.groups.len(),
        report_path,
    })
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

fn parse_config(text: &str) -> QualityResult<LeanProofRunnerConfig> {
    basic_toml::from_str(text)
        .map_err(|err| format!("{RUNNER_CONFIG_PATH}: failed to parse TOML: {err}"))
}

fn validate_config(config: &LeanProofRunnerConfig) -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_runner(&config.runner));
    diagnostics.extend(validate_guardrails(&config.guardrails));
    diagnostics.extend(validate_groups(&config.groups));
    diagnostics
}

fn validate_runner(runner: &RunnerConfig) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if !runner.lockstep_mode {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.lockstep_mode must be true for release checks"
        ));
    }
    if !runner.clean_env {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.clean_env must be true"
        ));
    }
    if !runner.forbidden_env.iter().any(|name| name == "LEAN_PATH") {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.forbidden_env must include LEAN_PATH"
        ));
    }
    if !runner.temp_root.starts_with("build/tmp/") {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.temp_root must stay under build/tmp/"
        ));
    }
    if runner.lean_version != "4.31.0" {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.lean_version must be `4.31.0`"
        ));
    }
    if runner.elan_channel != "leanprover/lean4:v4.31.0" {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.elan_channel must be `leanprover/lean4:v4.31.0`"
        ));
    }
    if runner.lake_flags != ["env", "lean"] {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.lake_flags must be exactly `env lean`"
        ));
    }
    if runner.dependency_lockfile != "proofs/lean/lake-manifest.json" {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: runner.dependency_lockfile must be `proofs/lean/lake-manifest.json`"
        ));
    }
    diagnostics
}

fn validate_guardrails(guardrails: &GuardrailConfig) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if guardrails.warning_wall_time_multiplier < 2 {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: warning wall-time multiplier must be at least 2"
        ));
    }
    if guardrails.hard_wall_time_multiplier < guardrails.warning_wall_time_multiplier {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: hard wall-time multiplier must be >= warning multiplier"
        ));
    }
    if !guardrails.closeout_requires_lockstep {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: closeout_requires_lockstep must be true"
        ));
    }
    diagnostics
}

fn validate_groups(groups: &[GroupConfig]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for group in groups {
        if !seen.insert(group.name.as_str()) {
            diagnostics.push(format!(
                "{RUNNER_CONFIG_PATH}: duplicate group `{}`",
                group.name
            ));
        }
        diagnostics.extend(validate_group(group));
    }
    for required in REQUIRED_GROUPS {
        if !seen.contains(required) {
            diagnostics.push(format!(
                "{RUNNER_CONFIG_PATH}: missing scheduling group `{required}`"
            ));
        }
    }
    diagnostics
}

fn validate_group(group: &GroupConfig) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if group.max_parallelism == 0 {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: group `{}` max_parallelism must be positive",
            group.name
        ));
    }
    if group.timeout_ms == 0 || group.cpu_ms == 0 || group.memory_mb == 0 || group.io_mb == 0 {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: group `{}` budgets must be positive",
            group.name
        ));
    }
    if group.retry_count > 2 {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: group `{}` retry_count must be <= 2",
            group.name
        ));
    }
    if group.name == "foundational" && (!group.lockstep || group.max_parallelism != 1) {
        diagnostics.push(format!(
            "{RUNNER_CONFIG_PATH}: foundational group must be lockstep with max_parallelism 1"
        ));
    }
    diagnostics
}

fn runtime_report(config: &LeanProofRunnerConfig) -> LeanProofRuntimeReport {
    LeanProofRuntimeReport {
        schema: "terlan.lean-proof-runtime-policy.v1",
        config_path: RUNNER_CONFIG_PATH,
        configured_policy: config.clone(),
    }
}

fn write_report(path: &Path, report: &LeanProofRuntimeReport) -> QualityResult<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("{}: failed to create directory: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(report)
        .map_err(|err| format!("{}: failed to serialize report: {err}", path.display()))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|err| format!("{}: failed to write report: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
#[path = "lean_proof_runtime_test.rs"]
#[cfg(test)]
mod lean_proof_runtime_test;
