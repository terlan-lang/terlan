use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{sha256_file, ArtifactRow};
use crate::terlan_quality::QualityResult;

const REPRO_REPORT_PATH: &str = "build/artifacts/lean-proof-repro-report.json";
const GATE_REPORT_PATH: &str = "build/artifacts/lean-proof-gate.json";
const BASELINE_PATH: &str = "build/artifacts/lean-proof-baseline.tsv";
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
struct OutputSignature {
    stdout_class: String,
    stderr_class: String,
    exit_class: String,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedExecution {
    exit: i32,
    stdout: String,
    stderr: String,
}

pub(super) fn run_proof_reproducibility(
    root: &Path,
    artifacts: &[ArtifactRow],
) -> QualityResult<Vec<ProofReplayVerdict>> {
    let current = artifacts
        .iter()
        .filter(|artifact| artifact.status == "current")
        .collect::<Vec<_>>();
    let mut verdicts = Vec::new();
    for artifact in current {
        let metadata = read_metadata(root, &artifact.replay_metadata)?;
        validate_metadata(root, artifact, &metadata)?;
        validate_toolchain(root, &metadata.toolchain)?;

        clean_proof_build_state(root, &metadata.family)?;
        let first_result = execute_metadata(root, &metadata);
        let first_cleanup = clean_proof_build_state(root, &metadata.family);
        let first = first_result?;
        first_cleanup?;
        let second_result = execute_metadata(root, &metadata);
        let second_cleanup = clean_proof_build_state(root, &metadata.family);
        let second = second_result?;
        second_cleanup?;
        let first_signature = execution_signature(&first);
        let second_signature = execution_signature(&second);
        if first_signature != second_signature {
            return Err(format!(
                "proof_gap[nondeterministic]: proof family `{}` produced signatures `{first_signature}` and `{second_signature}`; first={first:?}; second={second:?}; classify it as nondeterministic with a remediation plan",
                metadata.family,
            ));
        }
        validate_output_signature(&metadata, &first)?;
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
    let statuses = build_family_statuses(root, artifacts, &verdicts)?;
    write_reports(root, &verdicts, &statuses)?;
    Ok(verdicts)
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

fn validate_toolchain(root: &Path, toolchain: &ToolchainContract) -> QualityResult<()> {
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
    let output = Command::new("lake")
        .args(["env", "lean", "--version"])
        .current_dir(root.join("proofs/lean"))
        .env_remove("LEAN_PATH")
        .env("ELAN_NO_UPDATE_CHECK", "1")
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                "lean_unavailable: failed to launch pinned Lake/Lean toolchain".to_string()
            } else {
                format!("failed to launch pinned Lake/Lean toolchain: {err}")
            }
        })?;
    let version = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !version.contains(&format!("version {}", toolchain.lean_version))
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

fn clean_proof_build_state(root: &Path, family: &str) -> QualityResult<()> {
    for path in [
        root.join("proofs/lean/.lake/build"),
        root.join("proofs/lean/.lake/config"),
        root.join("build/tmp/lean-proof").join(family),
    ] {
        if path.exists() {
            fs::remove_dir_all(&path).map_err(|err| {
                format!(
                    "{}: failed to clean proof build state: {err}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn execute_metadata(root: &Path, metadata: &ReplayMetadata) -> QualityResult<NormalizedExecution> {
    let command = &metadata.execution_command;
    let output = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(root.join(&metadata.working_directory))
        .env_remove("LEAN_PATH")
        .env("ELAN_NO_UPDATE_CHECK", "1")
        .output()
        .map_err(|err| unavailable_error(&command[0], err))?;
    Ok(normalize_execution(root, output))
}

fn unavailable_error(command: &str, err: std::io::Error) -> String {
    if err.kind() == std::io::ErrorKind::NotFound {
        format!("lean_unavailable: failed to launch proof command `{command}`")
    } else {
        format!("failed to launch proof command `{command}`: {err}")
    }
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
    root: &Path,
    verdicts: &[ProofReplayVerdict],
    statuses: &[ProofFamilyStatus],
) -> QualityResult<()> {
    let repro_path = root.join(REPRO_REPORT_PATH);
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
    write_json(&repro_path, &repro)?;

    let gate_path = root.join(GATE_REPORT_PATH);
    let mut gate = if gate_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&gate_path).map_err(|err| {
            format!(
                "{}: failed to read Lean gate report: {err}",
                gate_path.display()
            )
        })?)
        .map_err(|err| {
            format!(
                "{}: invalid Lean gate report JSON: {err}",
                gate_path.display()
            )
        })?
    } else {
        json!({"schema": "terlan.lean-proof-gate.v1"})
    };
    let object = gate.as_object_mut().ok_or_else(|| {
        format!(
            "{}: Lean gate report must be a JSON object",
            gate_path.display()
        )
    })?;
    object.insert(
        "reproducibility".to_string(),
        serde_json::to_value(verdicts)
            .map_err(|err| format!("failed to serialize Lean reproducibility verdicts: {err}"))?,
    );
    object.insert(
        "families".to_string(),
        serde_json::to_value(statuses)
            .map_err(|err| format!("failed to serialize Lean family statuses: {err}"))?,
    );
    write_json(&gate_path, &gate)?;
    write_baseline(root, statuses)
}

fn write_baseline(root: &Path, statuses: &[ProofFamilyStatus]) -> QualityResult<()> {
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
    let path = root.join(BASELINE_PATH);
    fs::write(&path, text).map_err(|err| {
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
    let hexadecimal = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hexadecimal}")
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
