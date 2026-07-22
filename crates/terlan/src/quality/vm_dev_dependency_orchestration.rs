use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-dev-dependency-report.json";
const COMMAND_MAPPINGS: &[&str] = &[
    "serve",
    "db migrate",
    "db status",
    "db snapshot",
    "db rebuild --dev --confirm",
    "db reset --dev --confirm",
];
const DIAGNOSTICS: &[&str] = &[
    "dev_dependency.docker_missing",
    "dev_dependency.start_failed",
    "dev_dependency.readiness_failed",
    "dev_dependency.ownership_failed",
    "dev_dependency.stop_failed",
];
const DB_DEPENDENCY_PREPARE_CALL: &str = "prepare_local_database_dependencies(&directory, &config)";
const EXPECTED_DB_DEPENDENCY_PREPARE_CALLS: usize = 4;
const SOURCE_CONTRACTS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/dev_dependencies.rs",
        &[
            "docker_compose_types",
            "serde_yaml::from_str::<Compose>",
            "start_project_dependencies_for_path",
            "project_root_for_path",
            "--wait",
            "--wait-timeout",
            "--no-recreate",
            "docker_compose_logs_command",
            "--tail",
            "redact_compose_logs",
            "DevDependencySession",
            "DependencyOwnership::External",
            "DependencyOwnership::Owned",
            "docker_compose_inspect_command",
            "docker_compose_remove_command",
            "finish_dependency_session",
            "error[dev_dependency.docker_missing]",
            "error[dev_dependency.start_failed]",
            "error[dev_dependency.readiness_failed]",
            "dev_dependency.ownership_failed",
            "dev_dependency.stop_failed",
            "is_loopback_compose_host",
        ],
    ),
    (
        "crates/terlan/src/commands/db/mod.rs",
        &[
            "prepare_local_database_dependencies",
            "dev_dependencies::start_project_dependencies_for_path",
            "if !is_local_database_host(&target.host)",
            "finish_dependency_session(dependency_session, outcome)",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/mod.rs",
        &[
            "dev_dependencies::start_project_dependencies",
            "dev_dependencies::finish_dependency_session",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/manifest.rs",
        &["dev_dependencies::validate_project_compose"],
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmDevDependencyOrchestrationSummary {
    pub command_count: usize,
    pub diagnostic_count: usize,
    pub contract_fingerprint: String,
    pub report_path: PathBuf,
}

/// Validates the shared development-dependency foundation and writes evidence.
pub fn run_vm_dev_dependency_orchestration(
    root: &Path,
) -> QualityResult<VmDevDependencyOrchestrationSummary> {
    let mut diagnostics = Vec::new();
    let mut input_digests = BTreeMap::new();
    let mut contract_hasher = Sha256::new();
    for (relative, anchors) in SOURCE_CONTRACTS {
        let source = read(root, relative)?;
        record_input(relative, &source, &mut input_digests, &mut contract_hasher);
        for anchor in *anchors {
            if !source.contains(anchor) {
                diagnostics.push(format!(
                    "{relative}: missing VM development dependency anchor `{anchor}`"
                ));
            }
        }
        if *relative == "crates/terlan/src/commands/db/mod.rs" {
            let call_count = source.matches(DB_DEPENDENCY_PREPARE_CALL).count();
            if call_count != EXPECTED_DB_DEPENDENCY_PREPARE_CALLS {
                diagnostics.push(format!(
                    "{relative}: expected {EXPECTED_DB_DEPENDENCY_PREPARE_CALLS} local dependency preparation call sites, found {call_count}"
                ));
            }
        }
    }
    diagnostics.extend(validate_make_ownership(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let contract_fingerprint = hex_digest(contract_hasher.finalize().as_slice());
    let report = json!({
        "schema": "terlan.vm-dev-dependency-orchestration.v1",
        "gate_id": "vm-dev-dependency-orchestration-check",
        "input_digests": input_digests,
        "tool_versions": {"terlan": env!("CARGO_PKG_VERSION")},
        "environment": {
            "compose_parser": "docker-compose-types-plus-serde-yaml",
            "docker_execution": "direct-cli-without-shell",
            "credentials_reported": false,
            "compose_environment_reported": false,
        },
        "diagnostics": {"coverage": DIAGNOSTICS, "failures": []},
        "coverage_deltas": {"status": "foundation-covered"},
        "benchmark_data": null,
        "support_bundle_references": [],
        "decision": "pass",
        "release_blocking_rationale": "declared Compose dependencies are typed, shared, and admitted before local VM commands",
        "generated_at": null,
        "artifact_evidence": {
            "contract_fingerprint_sha256": contract_fingerprint,
            "command_mappings": COMMAND_MAPPINGS,
            "implemented_lifecycle": [
                "discover-nearest-project",
                "parse-typed-compose",
                "validate-postgres-contract",
                "start-project-owned-service",
                "wait-for-healthcheck",
                "reuse-running-service",
                "collect-logs",
                "redact-and-bound-log-excerpts",
                "probe-existing-container",
                "preserve-external",
                "stop-and-remove-owned",
            ],
            "remote_database_policy": "never-start-local-dependencies",
            "ownership_policy": "probe-before-start-and-remove-only-session-created-containers",
            "remaining_lifecycle": [
                "multi-service-dependency-graph",
            ],
            "generation_timestamp_policy": "omitted-for-determinism",
            "stable_ordering_policy": "static-command-order-and-btree-input-order",
            "path_redaction_policy": "repository-relative-inputs-only",
            "compatibility_policy": "exact-schema-v1",
        },
    });
    validate_report(&report)?;
    let report_path = write_report(root, &report)?;
    Ok(VmDevDependencyOrchestrationSummary {
        command_count: COMMAND_MAPPINGS.len(),
        diagnostic_count: DIAGNOSTICS.len(),
        contract_fingerprint,
        report_path,
    })
}

fn validate_make_ownership(root: &Path) -> QualityResult<Vec<String>> {
    let makefile = read(root, "Makefile")?;
    let required = [
        "vm-dev-dependency-orchestration-check:",
        "terlan-quality vm_dev_dependency_orchestration",
        "terlan-quality --quiet -- vm-dev-dependency-orchestration",
        "test -s target/quality/vm-dev-dependency-report.json",
        "vm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check",
    ];
    Ok(required
        .iter()
        .filter(|anchor| !makefile.contains(**anchor))
        .map(|anchor| format!("Makefile: missing VM dependency gate anchor `{anchor}`"))
        .collect())
}

fn validate_report(report: &serde_json::Value) -> QualityResult<()> {
    let text = serde_json::to_string(report)
        .map_err(|error| format!("{REPORT_PATH}: failed to validate report: {error}"))?;
    let forbidden = ["POSTGRES_PASSWORD", "postgres://", "password="];
    let leaked = forbidden
        .iter()
        .filter(|term| text.contains(**term))
        .copied()
        .collect::<Vec<_>>();
    if leaked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{REPORT_PATH}: report leaks dependency configuration: {}",
            leaked.join(", ")
        ))
    }
}

fn record_input(
    relative: &str,
    source: &str,
    input_digests: &mut BTreeMap<String, String>,
    contract_hasher: &mut Sha256,
) {
    let digest = sha256(source.as_bytes());
    input_digests.insert(relative.to_string(), format!("sha256:{digest}"));
    contract_hasher.update(relative.as_bytes());
    contract_hasher.update([0]);
    contract_hasher.update(source.as_bytes());
}

fn read(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|error| format!("{relative}: failed to read dependency evidence: {error}"))
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_report(root: &Path, report: &serde_json::Value) -> QualityResult<PathBuf> {
    let path = root.join(REPORT_PATH);
    let parent = path
        .parent()
        .ok_or_else(|| format!("{REPORT_PATH}: report path has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("{REPORT_PATH}: failed to create report directory: {error}"))?;
    let text = serde_json::to_string_pretty(report)
        .map_err(|error| format!("{REPORT_PATH}: failed to serialize report: {error}"))?;
    let temporary = parent.join(format!(
        ".vm-dev-dependency-report-{}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, format!("{text}\n"))
        .map_err(|error| format!("{REPORT_PATH}: failed to write report: {error}"))?;
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("{REPORT_PATH}: failed to publish report atomically: {error}")
    })?;
    Ok(path)
}

fn render_failure(diagnostics: &[String]) -> String {
    format!(
        "[vm-dev-dependency-orchestration] failures:\n  - {}",
        diagnostics.join("\n  - ")
    )
}

#[cfg(test)]
#[path = "vm_dev_dependency_orchestration_test.rs"]
mod vm_dev_dependency_orchestration_test;
