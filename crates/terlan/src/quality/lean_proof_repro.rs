use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::outputs::ReportPaths;
use super::{sha256_file, ArtifactRow};
use crate::commands::process_runner::run_command_with_timeout;
use crate::terlan_quality::QualityResult;

#[path = "lean_proof_workspace.rs"]
mod workspace;
use workspace::ProofWorkspace;

#[path = "lean_proof_environment.rs"]
mod environment;
use environment::ProofEnvironment;

#[cfg(target_os = "linux")]
#[path = "lean_proof_cached_replay.rs"]
mod cached;
#[cfg(target_os = "linux")]
#[path = "lean_proof_checkpoint.rs"]
mod checkpoint;
#[cfg(target_os = "linux")]
#[path = "lean_proof_tools.rs"]
mod tools;

#[cfg(target_os = "linux")]
#[path = "lean_proof_admission.rs"]
pub(super) mod admission;

const BASELINE_CLASSES: &[&str] = &[
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
    manifest_fingerprints: BTreeMap<String, String>,
    dependency_files: Vec<String>,
    proof_dependency_set_hash: String,
    source_digest: String,
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
    proof_digest: String,
    dependency_set_hash: String,
    first_signature: String,
    second_signature: String,
    verdict: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProofFamilyStatus {
    family: String,
    feature_class: String,
    theorem_identity: Vec<String>,
    proof_status: String,
    last_executed_digest: String,
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

/// Runs selected replicas without replacing the complete track's reports.
pub(super) fn run_replicas(
    root: &Path,
    artifacts: &[ArtifactRow],
) -> QualityResult<Vec<ProofReplayVerdict>> {
    replicas(root, artifacts, false)
}

/// Consumes completed replicas under fresh admission; never launches proof tools.
pub(super) fn require_replicas(
    root: &Path,
    artifacts: &[ArtifactRow],
) -> QualityResult<Vec<ProofReplayVerdict>> {
    replicas(root, artifacts, true)
}

fn replicas(
    root: &Path,
    artifacts: &[ArtifactRow],
    completed_only: bool,
) -> QualityResult<Vec<ProofReplayVerdict>> {
    #[cfg(not(target_os = "linux"))]
    if completed_only {
        return Err("completed proof consumption requires Linux checkpoint ownership".into());
    }
    let current = artifacts
        .iter()
        .filter(|artifact| artifact.status == "current")
        .map(|artifact| {
            let metadata = read_metadata(root, &artifact.replay_metadata)?;
            validate_metadata(root, artifact, &metadata)?;
            Ok((artifact, metadata))
        })
        .collect::<QualityResult<Vec<_>>>()?;
    let mut verdicts = Vec::new();
    let mut validated_toolchains = BTreeSet::new();
    let environment = ProofEnvironment::capture()?;
    #[cfg(target_os = "linux")]
    let admitted = admission::from_environment(
        root,
        &environment,
        &current
            .iter()
            .map(|(_, metadata)| metadata.toolchain.clone())
            .collect(),
    )?;
    #[cfg(target_os = "linux")]
    if completed_only && admitted.is_none() {
        return Err("completed proof consumption requires fresh tool admission".into());
    }
    #[cfg(target_os = "linux")]
    let mut pinned_tools = admitted.unwrap_or_default();
    #[cfg(target_os = "linux")]
    validated_toolchains.extend(pinned_tools.keys().cloned());
    #[cfg(not(target_os = "linux"))]
    if [
        "TERLAN_PROOF_TOOL_ADMISSION",
        "TERLAN_PROOF_TOOL_ADMISSION_SHA256",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some())
    {
        return Err("proof tool admission is only supported on Linux".into());
    }
    // Fail cheap contracts across every family before starting proof replicas.
    for (_, metadata) in &current {
        validate_toolchain_once(&mut validated_toolchains, &metadata.toolchain, || {
            validate_toolchain(root, &metadata.toolchain, &environment)?;
            #[cfg(target_os = "linux")]
            pinned_tools.insert(
                metadata.toolchain.clone(),
                tools::ProofTools::capture(root, &environment, &metadata.toolchain)?,
            );
            Ok(())
        })?;
    }
    #[cfg(target_os = "linux")]
    let mut checkpoints = cached::open(root, &current, &pinned_tools)?;
    for (artifact, metadata) in current {
        #[cfg(target_os = "linux")]
        let mut replica = 0;
        let (first_signature, second_signature) =
            validate_replay_pair(&metadata, artifact.expected_exit, || {
                #[cfg(target_os = "linux")]
                {
                    replica += 1;
                    cached::execute(
                        root,
                        &metadata,
                        artifact.expected_exit,
                        if completed_only {
                            None
                        } else {
                            Some(&environment)
                        },
                        &pinned_tools[&metadata.toolchain],
                        &mut checkpoints,
                        replica,
                    )
                }
                #[cfg(not(target_os = "linux"))]
                execute_metadata(root, &metadata, &environment)
            })?;
        verdicts.push(ProofReplayVerdict {
            family: metadata.family,
            proof_path: artifact.path.clone(),
            proof_digest: artifact.proof_digest.clone(),
            dependency_set_hash: metadata.proof_dependency_set_hash,
            first_signature,
            second_signature,
            verdict: "pass".to_string(),
        });
    }
    #[cfg(target_os = "linux")]
    eprintln!(
        "[proof-replay] {} completed replicas, {} verified reused replicas",
        checkpoints.completed, checkpoints.reused
    );
    Ok(verdicts)
}

fn validate_replay_pair(
    metadata: &ReplayMetadata,
    expected_exit: i32,
    mut execute: impl FnMut() -> QualityResult<NormalizedExecution>,
) -> QualityResult<(String, String)> {
    let first = execute()?;
    validate_execution(metadata, expected_exit, &first)?;
    let second = execute()?;
    validate_execution(metadata, expected_exit, &second)?;
    let first_signature = execution_signature(&first);
    let second_signature = execution_signature(&second);
    if first_signature != second_signature {
        return Err(format!(
            "proof_gap[nondeterministic]: proof family `{}` produced signatures `{first_signature}` and `{second_signature}`; first={first:?}; second={second:?}; classify it as nondeterministic with a remediation plan",
            metadata.family,
        ));
    }
    Ok((first_signature, second_signature))
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
            last_executed_digest: artifact.proof_digest.clone(),
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
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read replay metadata: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("{}: invalid replay metadata JSON: {err}", path.display()))
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
        diagnostics
            .push("proof path must be a repository-relative Lean source under proofs/lean".into());
    }
    if metadata.schema != "terlan.lean-proof-replay.v1" {
        diagnostics.push(format!("unsupported replay schema `{}`", metadata.schema));
    }
    if metadata.family.trim().is_empty() || metadata.theorem_names.is_empty() {
        diagnostics.push("replay metadata requires family and theorem names".to_string());
    }
    if metadata.deterministic_timestamp_strategy != "none-content-addressed" {
        diagnostics.push("timestamp strategy must be `none-content-addressed`".to_string());
    }
    if metadata.source_digest != artifact.proof_digest {
        diagnostics.push(format!(
            "proof_gap[artifact-drift]: replay source digest `{}` does not match artifact digest `{}`; update metadata or classify a blocker",
            metadata.source_digest, artifact.proof_digest
        ));
    }
    for manifest in &artifact.targeted_manifests {
        let expected = metadata.manifest_fingerprints.get(manifest);
        match sha256_file(&root.join(manifest)) {
            Ok(actual) if expected != Some(&actual) => {
                diagnostics.push(manifest_drift_diagnostic(
                    manifest,
                    expected.map(String::as_str),
                    &actual,
                ));
            }
            Err(err) => diagnostics.push(err),
            _ => {}
        }
    }
    match dependency_set_hash(root, &metadata.dependency_files) {
        Ok(actual) if actual != metadata.proof_dependency_set_hash => diagnostics.push(
            dependency_drift_diagnostic(&metadata.proof_dependency_set_hash, &actual),
        ),
        Err(err) => diagnostics.push(err),
        _ => {}
    }
    if metadata.execution_command.len() != 4
        || metadata.execution_command[0..3] != ["lake", "env", "lean"]
    {
        diagnostics.push("execution command must be `lake env lean <proof>`".to_string());
    }
    if metadata.working_directory != "proofs/lean" {
        diagnostics.push("proof working directory must be `proofs/lean`".to_string());
    }
    let expected_proof_argument = artifact
        .path
        .strip_prefix("proofs/lean/")
        .unwrap_or(&artifact.path);
    if metadata.execution_command.get(3).map(String::as_str) != Some(expected_proof_argument) {
        diagnostics.push(format!(
            "execution command proof argument must be `{expected_proof_argument}`"
        ));
    }
    let expected_exit_class = if artifact.expected_exit == 0 {
        "success"
    } else {
        "failure"
    };
    if metadata.output_signature.exit_class != expected_exit_class {
        diagnostics.push(format!(
            "artifact expected exit {} requires output exit class `{expected_exit_class}`",
            artifact.expected_exit
        ));
    }
    let expected_stderr_class = if artifact.stderr_class == "none" {
        "empty"
    } else {
        artifact.stderr_class.as_str()
    };
    if metadata.output_signature.stderr_class != expected_stderr_class {
        diagnostics.push(format!(
            "artifact stderr class `{}` requires output stderr class `{expected_stderr_class}`",
            artifact.stderr_class
        ));
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(render_failure(&metadata.family, &diagnostics))
    }
}

fn manifest_drift_diagnostic(manifest: &str, expected: Option<&str>, actual: &str) -> String {
    format!(
        "proof_gap[manifest-drift]: manifest fingerprint drift for `{manifest}`: expected `{}`, found `{actual}`",
        expected.unwrap_or("missing")
    )
}

fn dependency_drift_diagnostic(expected: &str, actual: &str) -> String {
    format!(
        "proof_gap[dependency-drift]: proof dependency set drift: expected `{expected}`, found `{actual}`"
    )
}

/// Identical families share one successful toolchain probe within this replay.
/// Failed probes are never recorded and distinct toolchain contracts cannot
/// reuse one another's validation.
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

fn validate_toolchain_inputs(root: &Path, toolchain: &ToolchainContract) -> QualityResult<String> {
    let channel = fs::read_to_string(root.join("proofs/lean/lean-toolchain"))
        .map_err(|err| format!("failed to read pinned Lean toolchain: {err}"))?;
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
    Ok(channel)
}

fn validate_toolchain(
    root: &Path,
    toolchain: &ToolchainContract,
    environment: &ProofEnvironment,
) -> QualityResult<()> {
    let channel = validate_toolchain_inputs(root, toolchain)?;
    let workspace = ProofWorkspace::create(root, &[])?;
    let copied_channel = fs::read_to_string(workspace.root().join("proofs/lean/lean-toolchain"))
        .map_err(|error| format!("cannot read private Lean toolchain pin: {error}"))?;
    if copied_channel != channel {
        return Err("Lean toolchain pin changed before isolated probe".into());
    }
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

fn dependency_set_hash(root: &Path, paths: &[String]) -> QualityResult<String> {
    let mut sorted = paths.to_vec();
    sorted.sort();
    if sorted != paths {
        return Err("proof dependency files must be byte-lexically sorted".to_string());
    }
    let mut hasher = Sha256::new();
    for relative in sorted {
        if !workspace::relative_input(&relative) {
            return Err(format!("unsafe proof dependency `{relative}`"));
        }
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(
            fs::read(root.join(&relative))
                .map_err(|err| format!("failed to read proof dependency `{relative}`: {err}"))?,
        );
    }
    let digest = hasher.finalize();
    Ok(format_digest(&digest))
}

#[cfg(not(target_os = "linux"))]
fn execute_metadata(
    root: &Path,
    metadata: &ReplayMetadata,
    environment: &ProofEnvironment,
) -> QualityResult<NormalizedExecution> {
    let command = &metadata.execution_command;
    let mut dependencies = metadata.dependency_files.clone();
    dependencies.extend(metadata.manifest_fingerprints.keys().cloned());
    let workspace = ProofWorkspace::create(root, &dependencies)?;
    validate_snapshot(workspace.root(), metadata)?;
    let mut process = Command::new(environment.program(&command[0])?);
    process
        .args(&command[1..])
        .current_dir(workspace.root().join(&metadata.working_directory));
    environment.configure(
        &mut process,
        workspace.root(),
        &metadata.toolchain.elan_channel,
    )?;
    let result = run_proof_command(&mut process, Duration::from_secs(600));
    let normalized = result.map(|output| normalize_execution(workspace.root(), output));
    let cleanup = workspace.close();
    let output = normalized?;
    cleanup?;
    Ok(output)
}

fn run_proof_command(command: &mut Command, timeout: Duration) -> QualityResult<Output> {
    run_command_with_timeout(command, "Lean proof process", timeout)
}

fn validate_snapshot(root: &Path, metadata: &ReplayMetadata) -> QualityResult<()> {
    let proof = root
        .join(&metadata.working_directory)
        .join(&metadata.execution_command[3]);
    if sha256_file(&proof)? != metadata.source_digest {
        return Err("proof source changed before isolated replay".into());
    }
    if dependency_set_hash(root, &metadata.dependency_files)? != metadata.proof_dependency_set_hash
    {
        return Err("proof dependencies changed before isolated replay".into());
    }
    for (path, expected) in &metadata.manifest_fingerprints {
        if sha256_file(&root.join(path))? != *expected {
            return Err(format!(
                "proof manifest changed before isolated replay: {path}"
            ));
        }
    }
    Ok(())
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

fn execution_signature(execution: &NormalizedExecution) -> String {
    let text = format!(
        "exit={}\nstdout={}\nstderr={}",
        execution.exit, execution.stdout, execution.stderr
    );
    let digest = Sha256::digest(text.as_bytes());
    format_digest(&digest)
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
            "proof family `{}` output signature mismatch: expected {}/{}/{}, found {actual_exit}/{actual_stdout}/{actual_stderr}",
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
    let repro_path = &paths.replay;
    if let Some(parent) = repro_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let repro = json!({
        "schema": "terlan.lean-proof-repro.v1",
        "timestamp_strategy": "none-content-addressed",
        "families": verdicts,
    });
    write_json(repro_path, &repro)?;

    // A producer never reads its previous output or the downstream lane seal.
    // Every field describes this execution; old lane checksums cannot survive.
    let track = json!({
        "schema": "terlan.lean-proof-track.v1",
        "reproducibility": verdicts,
        "families": statuses,
        "proof_gap_metrics": gap_metrics,
    });
    write_json(&paths.track, &track)?;
    write_baseline(&paths.baseline, statuses)
}

fn write_baseline(path: &Path, statuses: &[ProofFamilyStatus]) -> QualityResult<()> {
    let mut text = String::from("feature_class\texpected_status\tlast_confirmed_hash\n");
    for feature_class in BASELINE_CLASSES {
        let class_statuses = statuses
            .iter()
            .filter(|status| status.feature_class == *feature_class)
            .collect::<Vec<_>>();
        if class_statuses.is_empty() {
            text.push_str(&format!("{feature_class}\tincomplete\tnone\n"));
            continue;
        }
        let expected_status = class_statuses[0].proof_status.as_str();
        if class_statuses
            .iter()
            .any(|status| status.proof_status != expected_status)
        {
            return Err(format!(
                "proof class `{feature_class}` has mixed statuses and cannot produce a canonical baseline"
            ));
        }
        let hashes = class_statuses
            .iter()
            .map(|status| status.last_executed_digest.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(";");
        text.push_str(&format!("{feature_class}\t{expected_status}\t{hashes}\n"));
    }
    fs::write(path, text).map_err(|err| {
        format!(
            "{}: failed to write Lean proof baseline: {err}",
            path.display()
        )
    })
}

fn write_json(path: &Path, value: &Value) -> QualityResult<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| format!("{}: failed to serialize JSON: {err}", path.display()))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|err| format!("{}: failed to write JSON: {err}", path.display()))
}

fn format_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex_digest(bytes))
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
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
#[cfg(test)]
mod lean_proof_repro_test;
