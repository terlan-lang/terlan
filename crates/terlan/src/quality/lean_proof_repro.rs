use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::outputs::ReportPaths;
use super::ArtifactRow;
use crate::commands::process_runner::run_command_with_timeout;
use crate::terlan_quality::QualityResult;

#[path = "lean_proof_workspace.rs"]
mod workspace;
use workspace::ProofWorkspace;

#[path = "lean_proof_environment.rs"]
mod environment;
use environment::ProofEnvironment;

const STATUS_CLASSES: &[&str] = &[
    "coreir",
    "lowering",
    "rejection",
    "runtime",
    "vm",
    "native-boundary",
    "parser",
    "wasm",
    "aeneas-bridge",
];

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ReplayMetadata {
    schema: String,
    family: String,
    theorem_names: Vec<String>,
    manifests: Vec<String>,
    dependency_files: Vec<String>,
    execution_command: Vec<String>,
    working_directory: String,
    deterministic_timestamp_strategy: String,
    output_signature: OutputSignature,
    toolchain: ToolchainContract,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OutputSignature {
    stdout_class: String,
    stderr_class: String,
    exit_class: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
struct ToolchainContract {
    lean_version: String,
    elan_channel: String,
    lake_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct ProofReplayVerdict {
    family: String,
    proof_path: String,
    first_execution: NormalizedExecution,
    second_execution: NormalizedExecution,
    verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProofFamilyStatus {
    family: String,
    feature_class: String,
    theorem_identity: Vec<String>,
    proof_status: String,
    reproducibility_verdict: String,
    blockers: Vec<String>,
    remediation_gates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NormalizedExecution {
    exit: i32,
    stdout: String,
    stderr: String,
}

pub(super) fn run_proof_reproducibility(
    root: &Path,
    artifacts: &[ArtifactRow],
    gap_metrics: &Value,
    outputs: &ReportPaths,
) -> QualityResult<Vec<ProofReplayVerdict>> {
    let verdicts = run_replicas(root, artifacts)?;
    let statuses = build_family_statuses(root, artifacts, &verdicts)?;
    write_reports(outputs, &verdicts, &statuses, gap_metrics)?;
    Ok(verdicts)
}

pub(super) fn run_replicas(
    root: &Path,
    artifacts: &[ArtifactRow],
) -> QualityResult<Vec<ProofReplayVerdict>> {
    replicas(root, artifacts)
}

pub(super) fn require_replicas(
    root: &Path,
    artifacts: &[ArtifactRow],
) -> QualityResult<Vec<ProofReplayVerdict>> {
    replicas(root, artifacts)
}

fn replicas(root: &Path, artifacts: &[ArtifactRow]) -> QualityResult<Vec<ProofReplayVerdict>> {
    let current = artifacts
        .iter()
        .filter(|artifact| artifact.status == "current")
        .map(|artifact| {
            let metadata = read_metadata(root, &artifact.replay_metadata)?;
            validate_metadata(root, artifact, &metadata)?;
            Ok((artifact, metadata))
        })
        .collect::<QualityResult<Vec<_>>>()?;
    let environment = ProofEnvironment::capture()?;
    let mut validated_toolchains = BTreeSet::new();
    for (_, metadata) in &current {
        validate_toolchain_once(&mut validated_toolchains, &metadata.toolchain, || {
            validate_toolchain(root, &metadata.toolchain, &environment)
        })?;
    }
    current
        .into_iter()
        .map(|(artifact, metadata)| {
            let (first_execution, second_execution) =
                validate_replay_pair(&metadata, artifact.expected_exit, || {
                    execute_metadata(root, &metadata, &environment)
                })?;
            Ok(ProofReplayVerdict {
                family: metadata.family,
                proof_path: artifact.path.clone(),
                first_execution,
                second_execution,
                verdict: "pass".to_string(),
            })
        })
        .collect()
}

fn validate_replay_pair(
    metadata: &ReplayMetadata,
    expected_exit: i32,
    mut execute: impl FnMut() -> QualityResult<NormalizedExecution>,
) -> QualityResult<(NormalizedExecution, NormalizedExecution)> {
    let first = execute()?;
    validate_execution(metadata, expected_exit, &first)?;
    let second = execute()?;
    validate_execution(metadata, expected_exit, &second)?;
    if first != second {
        return Err(format!(
            "proof_gap[nondeterministic]: proof family `{}` produced different executions; first={first:?}; second={second:?}",
            metadata.family,
        ));
    }
    Ok((first, second))
}

fn validate_execution(
    metadata: &ReplayMetadata,
    expected_exit: i32,
    execution: &NormalizedExecution,
) -> QualityResult<()> {
    if execution.exit < 0 || execution.exit != expected_exit {
        return Err(format!(
            "proof family `{}` exit mismatch: expected {expected_exit}, found {}; signals are not proof verdicts",
            metadata.family, execution.exit
        ));
    }
    validate_output_signature(metadata, execution)
}

fn build_family_statuses(
    root: &Path,
    artifacts: &[ArtifactRow],
    verdicts: &[ProofReplayVerdict],
) -> QualityResult<Vec<ProofFamilyStatus>> {
    let mut statuses = Vec::new();
    for artifact in artifacts {
        let metadata = read_metadata(root, &artifact.replay_metadata)?;
        let verdict = verdicts
            .iter()
            .find(|verdict| verdict.proof_path == artifact.path);
        let current_pass = artifact.status == "current"
            && verdict.map(|verdict| verdict.verdict.as_str()) == Some("pass");
        statuses.push(ProofFamilyStatus {
            family: metadata.family,
            feature_class: feature_class(&artifact.theorem_scope).to_string(),
            theorem_identity: metadata.theorem_names,
            proof_status: artifact.status.clone(),
            reproducibility_verdict: verdict
                .map(|verdict| verdict.verdict.clone())
                .unwrap_or_else(|| "not-run".to_string()),
            blockers: if current_pass {
                Vec::new()
            } else {
                vec![format!("proof_gap[{}]", artifact.status)]
            },
            remediation_gates: if current_pass {
                Vec::new()
            } else {
                vec!["proof_repro_check".to_string()]
            },
        });
    }
    statuses.sort_by(|left, right| left.family.cmp(&right.family));
    Ok(statuses)
}

fn feature_class(scope: &str) -> &str {
    match scope {
        "CoreIR" => "coreir",
        "lowering" => "lowering",
        "rejection" => "rejection",
        "NativeBoundary" => "native-boundary",
        "parser" => "parser",
        _ => "runtime",
    }
}

fn read_metadata(root: &Path, relative: &str) -> QualityResult<ReplayMetadata> {
    let path = root.join(relative);
    let text = fs::read_to_string(&path).map_err(|error| {
        format!(
            "{}: failed to read replay metadata: {error}",
            path.display()
        )
    })?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{}: invalid replay metadata JSON: {error}", path.display()))
}

fn validate_metadata(
    root: &Path,
    artifact: &ArtifactRow,
    metadata: &ReplayMetadata,
) -> QualityResult<()> {
    let mut diagnostics = Vec::new();
    if !workspace::relative_input(&artifact.path)
        || !artifact.path.starts_with("proofs/lean/")
        || !artifact.path.ends_with(".lean")
    {
        diagnostics.push("proof path must be a repository-relative Lean source".to_string());
    }
    if metadata.schema != "terlan.lean-proof-replay.v2" {
        diagnostics.push(format!("unsupported replay schema `{}`", metadata.schema));
    }
    if metadata.family.trim().is_empty() || metadata.theorem_names.is_empty() {
        diagnostics.push("replay metadata requires family and theorem names".to_string());
    }
    if metadata.deterministic_timestamp_strategy != "none" {
        diagnostics.push("timestamp strategy must be `none`".to_string());
    }
    let mut declared_manifests = metadata.manifests.clone();
    declared_manifests.sort();
    let mut targeted_manifests = artifact.targeted_manifests.clone();
    targeted_manifests.sort();
    if declared_manifests != targeted_manifests {
        diagnostics.push("replay manifests must match the artifact manifest list".to_string());
    }
    for path in metadata
        .manifests
        .iter()
        .chain(metadata.dependency_files.iter())
    {
        if !workspace::relative_input(path) || !root.join(path).is_file() {
            diagnostics.push(format!("proof input is missing or unsafe: {path}"));
        }
    }
    if metadata.execution_command.len() != 4
        || metadata.execution_command[0..3] != ["lake", "env", "lean"]
    {
        diagnostics.push("execution command must be `lake env lean <proof>`".to_string());
    }
    if metadata.working_directory != "proofs/lean" {
        diagnostics.push("proof working directory must be `proofs/lean`".to_string());
    }
    let expected_argument = artifact
        .path
        .strip_prefix("proofs/lean/")
        .unwrap_or(&artifact.path);
    if metadata.execution_command.get(3).map(String::as_str) != Some(expected_argument) {
        diagnostics.push(format!(
            "execution command proof argument must be `{expected_argument}`"
        ));
    }
    let expected_exit_class = if artifact.expected_exit == 0 {
        "success"
    } else {
        "failure"
    };
    if metadata.output_signature.exit_class != expected_exit_class {
        diagnostics.push(format!(
            "artifact expected exit {} requires output class `{expected_exit_class}`",
            artifact.expected_exit
        ));
    }
    let expected_stderr = if artifact.stderr_class == "none" {
        "empty"
    } else {
        artifact.stderr_class.as_str()
    };
    if metadata.output_signature.stderr_class != expected_stderr {
        diagnostics.push(format!(
            "artifact stderr class requires output class `{expected_stderr}`"
        ));
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(render_failure(&metadata.family, &diagnostics))
    }
}

fn validate_toolchain_once(
    validated: &mut BTreeSet<ToolchainContract>,
    toolchain: &ToolchainContract,
    probe: impl FnOnce() -> QualityResult<()>,
) -> QualityResult<()> {
    if !validated.contains(toolchain) {
        probe()?;
        validated.insert(toolchain.clone());
    }
    Ok(())
}

fn validate_toolchain(
    root: &Path,
    toolchain: &ToolchainContract,
    environment: &ProofEnvironment,
) -> QualityResult<()> {
    let channel = fs::read_to_string(root.join("proofs/lean/lean-toolchain"))
        .map_err(|error| format!("failed to read pinned Lean toolchain: {error}"))?;
    if channel.trim() != toolchain.elan_channel {
        return Err(format!(
            "pinned Elan channel mismatch: expected `{}`, found `{}`",
            toolchain.elan_channel,
            channel.trim()
        ));
    }
    if toolchain.lake_flags != ["env", "lean"] {
        return Err("replay Lake flags must be exactly `env lean`".to_string());
    }
    let workspace = ProofWorkspace::create(root, &[])?;
    let mut command = Command::new(environment.program("lake")?);
    command
        .args(["env", "lean", "--version"])
        .current_dir(workspace.root().join("proofs/lean"));
    environment.configure(&mut command, workspace.root(), &toolchain.elan_channel)?;
    let result = run_proof_command(&mut command, Duration::from_secs(30));
    let cleanup = workspace.close();
    let output = result?;
    cleanup?;
    let version = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !environment::matches_version(&version, &toolchain.lean_version)
    {
        return Err(format!(
            "pinned Lean version mismatch: expected `{}`, output `{}`",
            toolchain.lean_version,
            version.trim()
        ));
    }
    Ok(())
}

fn execute_metadata(
    root: &Path,
    metadata: &ReplayMetadata,
    environment: &ProofEnvironment,
) -> QualityResult<NormalizedExecution> {
    let mut inputs = metadata.dependency_files.clone();
    inputs.extend(metadata.manifests.iter().cloned());
    let workspace = ProofWorkspace::create(root, &inputs)?;
    let mut command = Command::new(environment.program(&metadata.execution_command[0])?);
    command
        .args(&metadata.execution_command[1..])
        .current_dir(workspace.root().join(&metadata.working_directory));
    environment.configure(
        &mut command,
        workspace.root(),
        &metadata.toolchain.elan_channel,
    )?;
    let result = run_proof_command(&mut command, Duration::from_secs(600));
    let normalized = result.map(|output| normalize_execution(workspace.root(), output));
    let cleanup = workspace.close();
    let output = normalized?;
    cleanup?;
    Ok(output)
}

fn run_proof_command(command: &mut Command, timeout: Duration) -> QualityResult<Output> {
    run_command_with_timeout(command, "Lean proof process", timeout)
}

fn normalize_execution(root: &Path, output: Output) -> NormalizedExecution {
    normalized_execution_from_parts(
        root,
        output.status.code().unwrap_or(-1),
        output.stdout,
        output.stderr,
    )
}

fn normalized_execution_from_parts(
    root: &Path,
    exit: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> NormalizedExecution {
    let root_text = root.to_string_lossy();
    let normalize = |bytes: Vec<u8>| {
        String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .replace(root_text.as_ref(), "<repo>")
            .trim()
            .to_string()
    };
    NormalizedExecution {
        exit,
        stdout: normalize(stdout),
        stderr: normalize(stderr),
    }
}

fn validate_output_signature(
    metadata: &ReplayMetadata,
    execution: &NormalizedExecution,
) -> QualityResult<()> {
    let actual_exit = if execution.exit == 0 {
        "success"
    } else {
        "failure"
    };
    let actual_stdout = if execution.stdout.is_empty() {
        "empty"
    } else {
        "text"
    };
    let actual_stderr = if execution.stderr.is_empty() {
        "empty"
    } else {
        "text"
    };
    if metadata.output_signature.exit_class != actual_exit
        || metadata.output_signature.stdout_class != actual_stdout
        || metadata.output_signature.stderr_class != actual_stderr
    {
        return Err(format!(
            "proof family `{}` output class mismatch: expected {}/{}/{}, found {actual_exit}/{actual_stdout}/{actual_stderr}",
            metadata.family,
            metadata.output_signature.exit_class,
            metadata.output_signature.stdout_class,
            metadata.output_signature.stderr_class
        ));
    }
    Ok(())
}

fn write_reports(
    paths: &ReportPaths,
    verdicts: &[ProofReplayVerdict],
    statuses: &[ProofFamilyStatus],
    gap_metrics: &Value,
) -> QualityResult<()> {
    if let Some(parent) = paths.replay.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "{}: failed to create report directory: {error}",
                parent.display()
            )
        })?;
    }
    write_json(
        &paths.replay,
        &json!({
            "schema": "terlan.lean-proof-repro.v2",
            "timestamp_strategy": "none",
            "families": verdicts,
        }),
    )?;
    write_json(
        &paths.track,
        &json!({
            "schema": "terlan.lean-proof-track.v2",
            "reproducibility": verdicts,
            "families": statuses,
            "proof_gap_metrics": gap_metrics,
        }),
    )?;
    write_statuses(&paths.baseline, statuses)
}

fn write_statuses(path: &Path, statuses: &[ProofFamilyStatus]) -> QualityResult<()> {
    let mut text = String::from("feature_class\texpected_status\n");
    for feature_class in STATUS_CLASSES {
        let class_statuses = statuses
            .iter()
            .filter(|status| status.feature_class == *feature_class)
            .collect::<Vec<_>>();
        if class_statuses.is_empty() {
            text.push_str(&format!("{feature_class}\tincomplete\n"));
            continue;
        }
        let expected_status = class_statuses[0].proof_status.as_str();
        if class_statuses
            .iter()
            .any(|status| status.proof_status != expected_status)
        {
            return Err(format!(
                "proof class `{feature_class}` has mixed statuses and cannot produce a status report"
            ));
        }
        text.push_str(&format!("{feature_class}\t{expected_status}\n"));
    }
    fs::write(path, text).map_err(|error| {
        format!(
            "{}: failed to write Lean proof status: {error}",
            path.display()
        )
    })
}

fn write_json(path: &Path, value: &Value) -> QualityResult<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("{}: failed to serialize JSON: {error}", path.display()))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("{}: failed to write JSON: {error}", path.display()))
}

fn render_failure(family: &str, diagnostics: &[String]) -> String {
    format!(
        "proof replay metadata `{family}` failed:\n{}",
        diagnostics
            .iter()
            .map(|diagnostic| format!("- {diagnostic}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[cfg(test)]
#[path = "lean_proof_repro_test.rs"]
mod tests;
