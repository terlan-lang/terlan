use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::terlan_quality::lean_proof_track::lean_proof_gap::{
    current_utc_date, parse_gap_manifest, read_gap_policy, validate_gap_lifecycle,
    validate_gap_toml_mirror, write_gap_metrics, LeanProofGap, GAP_PATH,
};
use crate::terlan_quality::QualityResult;

#[path = "lean_proof_gap.rs"]
pub(crate) mod lean_proof_gap;

#[path = "lean_proof_gap_hygiene.rs"]
pub(crate) mod gap_hygiene;

#[path = "lean_proof_gap_transition.rs"]
pub(crate) mod gap_transition;

#[path = "lean_proof_repro.rs"]
mod lean_proof_repro;

pub(super) const TRACK_DOC: &str = "docs/compiler/LEAN_PROOF_TRACK.md";
pub(super) const INVENTORY_PATH: &str = "docs/compiler/proof_track/lean_proof_inventory.tsv";
const PROOF_ROOT: &str = "proofs/lean";
pub(super) const ARTIFACT_PATH: &str = "proofs/lean/ci/lean-proof-artifacts.tsv";

const INVENTORY_HEADER: &str = "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes";
const ARTIFACT_HEADER: &str =
    "path\tstatus\ttheorem_scope\ttargeted_manifests\texpected_exit\tstderr_class\tproof_digest\treplay_metadata\tremediation_plan";
const VALID_THEOREM_SCOPES: &[&str] = &[
    "CoreIR",
    "NativeBoundary",
    "lowering",
    "parser",
    "rejection",
];
const REQUIRED_CURRENT_THEOREM_SCOPES: &[&str] = &["CoreIR", "lowering", "rejection"];

const VALID_INVENTORY_STATUSES: &[&str] = &[
    "current",
    "stale",
    "incomplete",
    "nondeterministic",
    "generated-only",
    "delete-candidate",
    "absent",
];

const REQUIRED_GAPS: &[&str] = &[
    "EBNF syntax preservation",
    "typed CoreIR preservation",
    "target-profile inference",
    "VM execution subset",
    "pattern and operator coverage",
    "native-boundary contracts",
    "Wasm CoreIR lowering",
    "Aeneas Rust verification bridge",
];

const STALE_TERMS: &[&str] = &[
    "CoreV0",
    "core-v0",
    "BEAM lowering",
    "Erlang lowering",
    "@target.erlang",
    "otp_application",
];

const VALID_GAP_OWNERS: &[&str] = &["compiler", "formal", "runtime", "vm"];

const REQUIRED_COVERAGE_MANIFESTS: &[(&str, &str)] = &[
    (
        "docs/grammar/TERLAN_SYNTAX_SPEC.ebnf",
        "EBNF syntax preservation",
    ),
    (
        "docs/compiler/type_spec/terlan_core_typing_spec.toml",
        "typed CoreIR preservation",
    ),
    (
        "docs/compiler/CORE_IR_LEAN_CONFORMANCE.md",
        "typed CoreIR preservation",
    ),
    (
        "docs/compiler/TERLAN_TARGET_INFERENCE.md",
        "target-profile inference",
    ),
    (
        "docs/runtime/TERLAN_VM_RUNTIME_CONCEPTS.md",
        "VM execution subset",
    ),
    (
        "docs/compiler/type_spec/language_feature_coverage_matrix.json",
        "pattern and operator coverage",
    ),
    (
        "docs/compiler/type_spec/operator_coverage_matrix.json",
        "pattern and operator coverage",
    ),
    (
        "docs/compiler/type_spec/pattern_matching_support_matrix.json",
        "pattern and operator coverage",
    ),
    (
        "docs/compiler/type_spec/binary_descriptor_matrix.json",
        "pattern and operator coverage",
    ),
    (
        "docs/runtime/NATIVE_BOUNDARY_GLOSSARY.md",
        "native-boundary contracts",
    ),
    (
        "crates/terlan/src/backends/wasm/README.md",
        "Wasm CoreIR lowering",
    ),
    (
        "docs/compiler/LEAN_PROOF_TRACK.md",
        "Aeneas Rust verification bridge",
    ),
];

/// Summary produced by the Lean proof-track quality gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanProofTrackSummary {
    pub inventory_row_count: usize,
    pub gap_row_count: usize,
    pub lean_file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InventoryRow {
    pub(super) path: String,
    pub(super) status: String,
    pub(super) source_contract: String,
    terlan_version: String,
    gate: String,
    notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ArtifactRow {
    path: String,
    status: String,
    theorem_scope: String,
    targeted_manifests: Vec<String>,
    expected_exit: i32,
    stderr_class: String,
    proof_digest: String,
    replay_metadata: String,
    remediation_plan: String,
}

/// Runs the Lean proof-track inventory and gap-manifest gate.
///
/// Inputs:
/// - `root`: repository root containing compiler docs and optional
///   `proofs/lean`.
///
/// Output:
/// - Success summary with inventory, gap, and Lean-file counts.
/// - Stable diagnostics for missing manifests, stale terminology, untracked
///   proof files, or unclassified proof gaps.
///
/// Transformation:
/// - Makes formal-proof status executable without pretending the current
///   release ships complete Lean proof coverage.
pub fn run_lean_proof_track(root: &Path) -> QualityResult<LeanProofTrackSummary> {
    let track_doc = read_text(root, TRACK_DOC)?;
    let inventory_text = read_text(root, INVENTORY_PATH)?;
    let gap_text = read_text(root, GAP_PATH)?;
    let artifact_text = read_text(root, ARTIFACT_PATH)?;
    let lean_files = collect_lean_files(root)?;
    let make_check_targets = collect_make_check_targets(root)?;

    let mut diagnostics = validate_track_doc(&track_doc);
    diagnostics.extend(validate_stale_terms(TRACK_DOC, &track_doc));
    diagnostics.extend(validate_stale_terms(INVENTORY_PATH, &inventory_text));
    diagnostics.extend(validate_stale_terms(GAP_PATH, &gap_text));

    let inventory_rows = parse_inventory(&inventory_text)?;
    let gap_rows = parse_gaps(&gap_text)?;
    let gap_policy = read_gap_policy(root)?;
    let today = current_utc_date();
    let artifact_rows = parse_artifacts(&artifact_text)?;
    diagnostics.extend(validate_inventory(&inventory_rows, &lean_files));
    diagnostics.extend(validate_gaps(&gap_rows, &make_check_targets));
    diagnostics.extend(validate_gap_lifecycle(&gap_rows, &gap_policy, today));
    diagnostics.extend(validate_gap_toml_mirror(root, &gap_rows, today)?);
    diagnostics.extend(validate_gap_manifest_paths(root, &gap_rows));
    diagnostics.extend(validate_lean_files(root, &lean_files));
    diagnostics.extend(validate_artifacts(root, &artifact_rows, &inventory_rows));

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    lean_proof_repro::run_proof_reproducibility(root, &artifact_rows)?;
    write_gap_metrics(root, &gap_rows, &gap_policy, today)?;

    Ok(LeanProofTrackSummary {
        inventory_row_count: inventory_rows.len(),
        gap_row_count: gap_rows.len(),
        lean_file_count: lean_files.len(),
    })
}

fn validate_track_doc(doc: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for term in [
        "make lean-proof-track-check",
        "lean_proof_inventory.tsv",
        "lean_proof_gaps.tsv",
        "lean-proof-artifacts.tsv",
        "Aeneas/Rust verification",
    ] {
        if !doc.contains(term) {
            diagnostics.push(format!("`{TRACK_DOC}` is missing required term `{term}`"));
        }
    }
    diagnostics
}

pub(super) fn parse_inventory(text: &str) -> QualityResult<Vec<InventoryRow>> {
    let rows = parse_tsv(text, INVENTORY_HEADER, INVENTORY_PATH)?;
    Ok(rows
        .into_iter()
        .map(|columns| InventoryRow {
            path: columns[0].clone(),
            status: columns[1].clone(),
            source_contract: columns[2].clone(),
            terlan_version: columns[3].clone(),
            gate: columns[4].clone(),
            notes: columns[5].clone(),
        })
        .collect())
}

fn parse_gaps(text: &str) -> QualityResult<Vec<LeanProofGap>> {
    parse_gap_manifest(text)
}

pub(super) fn parse_artifacts(text: &str) -> QualityResult<Vec<ArtifactRow>> {
    parse_tsv(text, ARTIFACT_HEADER, ARTIFACT_PATH)?
        .into_iter()
        .map(|columns| {
            let expected_exit = columns[4].parse::<i32>().map_err(|err| {
                format!(
                    "`{ARTIFACT_PATH}` row `{}` has invalid expected exit `{}`: {err}",
                    columns[0], columns[4]
                )
            })?;
            Ok(ArtifactRow {
                path: columns[0].clone(),
                status: columns[1].clone(),
                theorem_scope: columns[2].clone(),
                targeted_manifests: split_manifest_list(&columns[3]),
                expected_exit,
                stderr_class: columns[5].clone(),
                proof_digest: columns[6].clone(),
                replay_metadata: columns[7].clone(),
                remediation_plan: columns[8].clone(),
            })
        })
        .collect()
}

fn split_manifest_list(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_tsv(text: &str, header: &str, path: &str) -> QualityResult<Vec<Vec<String>>> {
    let mut lines = text.lines();
    match lines.next() {
        Some(first) if first == header => {}
        Some(first) => {
            return Err(format!(
                "`{path}` header mismatch: expected `{header}`, found `{first}`"
            ));
        }
        None => return Err(format!("`{path}` is empty")),
    }

    let width = header.split('\t').count();
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns = line
            .split('\t')
            .map(str::to_string)
            .collect::<Vec<String>>();
        if columns.len() != width {
            return Err(format!(
                "`{path}` row {} has {} columns, expected {width}",
                index + 2,
                columns.len()
            ));
        }
        rows.push(columns);
    }
    Ok(rows)
}

fn validate_inventory(rows: &[InventoryRow], lean_files: &[PathBuf]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if rows.is_empty() {
        diagnostics.push(format!("`{INVENTORY_PATH}` must contain at least one row"));
    }

    let mut inventory_paths = BTreeSet::new();
    for row in rows {
        if !inventory_paths.insert(row.path.clone()) {
            diagnostics.push(format!("duplicate Lean proof inventory row `{}`", row.path));
        }
        for (field, value) in [
            ("path", row.path.as_str()),
            ("status", row.status.as_str()),
            ("source_contract", row.source_contract.as_str()),
            ("terlan_version", row.terlan_version.as_str()),
            ("gate", row.gate.as_str()),
            ("notes", row.notes.as_str()),
        ] {
            if value.trim().is_empty() {
                diagnostics.push(format!(
                    "`{INVENTORY_PATH}` row `{}` has empty `{field}`",
                    row.path
                ));
            }
        }
        if !VALID_INVENTORY_STATUSES.contains(&row.status.as_str()) {
            diagnostics.push(format!(
                "`{INVENTORY_PATH}` row `{}` has invalid status `{}`",
                row.path, row.status
            ));
        }
        if row.terlan_version != "0.0.7" {
            diagnostics.push(format!(
                "`{INVENTORY_PATH}` row `{}` targets `{}`, expected `0.0.7`",
                row.path, row.terlan_version
            ));
        }
        if row.gate != "lean-proof-track-check" {
            diagnostics.push(format!(
                "`{INVENTORY_PATH}` row `{}` is owned by `{}`, expected `lean-proof-track-check`",
                row.path, row.gate
            ));
        }
    }

    if lean_files.is_empty() {
        for row in rows {
            if row.status != "absent" {
                diagnostics.push(format!(
                    "`{INVENTORY_PATH}` row `{}` claims status `{}` but no Lean proof files exist",
                    row.path, row.status
                ));
            }
        }
        if !rows
            .iter()
            .any(|row| row.path == PROOF_ROOT && row.status == "absent")
        {
            diagnostics.push(format!(
                "`{INVENTORY_PATH}` must include an absent `{PROOF_ROOT}` row when no Lean files exist"
            ));
        }
    } else {
        let lean_paths = lean_files
            .iter()
            .map(|file| normalize_path(file))
            .collect::<BTreeSet<String>>();
        for file in lean_files {
            let text = normalize_path(file);
            if !inventory_paths.contains(&text) {
                diagnostics.push(format!(
                    "Lean proof file `{text}` is missing from `{INVENTORY_PATH}`"
                ));
            }
        }
        for row in rows {
            if row.status == "absent" {
                diagnostics.push(format!(
                    "`{INVENTORY_PATH}` row `{}` is stale: Lean proof files exist",
                    row.path
                ));
            } else if !lean_paths.contains(&row.path) {
                diagnostics.push(format!(
                    "`{INVENTORY_PATH}` row `{}` references a missing Lean proof file",
                    row.path
                ));
            }
        }
    }

    diagnostics
}

pub(super) fn validate_gaps(
    rows: &[LeanProofGap],
    make_check_targets: &BTreeSet<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut features = BTreeSet::new();
    for row in rows {
        if !features.insert(row.feature.clone()) {
            diagnostics.push(format!("duplicate Lean proof gap row `{}`", row.feature));
        }
        for (field, value) in [
            ("feature", row.feature.as_str()),
            ("reason", row.reason.as_str()),
            ("lifecycle_status", row.lifecycle_status.as_str()),
            ("proof_gap_category", row.category.as_str()),
            ("remediation_owner", row.remediation_owner.as_str()),
            ("planned_gate", row.planned_gate.as_str()),
            ("deadline_or_exception", row.deadline_or_exception.as_str()),
            ("blocker_hash", row.blocker_hash.as_str()),
        ] {
            if value.trim().is_empty() {
                diagnostics.push(format!(
                    "`{GAP_PATH}` row `{}` has empty `{field}`",
                    row.feature
                ));
            }
        }
        if row.covered_manifests.is_empty() {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` must list at least one covered manifest",
                row.feature
            ));
        }
        if !row.planned_gate.ends_with("-check") {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` planned gate `{}` must be a Make-style `*-check` target",
                row.feature, row.planned_gate
            ));
        }
        if !make_check_targets.contains(&row.planned_gate) {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` planned gate `{}` is not defined by Make targets",
                row.feature, row.planned_gate
            ));
        }
        if !VALID_GAP_OWNERS.contains(&row.remediation_owner.as_str()) {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` owner `{}` is not an accepted proof owner",
                row.feature, row.remediation_owner
            ));
        }
    }
    for required in REQUIRED_GAPS {
        if !features.contains(*required) {
            diagnostics.push(format!("`{GAP_PATH}` is missing required gap `{required}`"));
        }
    }
    diagnostics.extend(validate_required_manifest_links(rows));
    diagnostics
}

fn validate_required_manifest_links(rows: &[LeanProofGap]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (manifest, expected_feature) in REQUIRED_COVERAGE_MANIFESTS {
        let covered = rows.iter().any(|row| {
            row.feature == *expected_feature
                && row
                    .covered_manifests
                    .iter()
                    .any(|covered_manifest| covered_manifest == manifest)
        });
        if !covered {
            diagnostics.push(format!(
                "`{GAP_PATH}` must link `{manifest}` to proof gap `{expected_feature}`"
            ));
        }
    }
    diagnostics
}

fn validate_gap_manifest_paths(root: &Path, rows: &[LeanProofGap]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        for manifest in &row.covered_manifests {
            if !seen.insert(manifest.clone()) {
                continue;
            }
            if !root.join(manifest).exists() {
                diagnostics.push(format!(
                    "`{GAP_PATH}` row `{}` references missing covered manifest `{manifest}`",
                    row.feature
                ));
            }
        }
    }
    diagnostics
}

pub(super) fn collect_make_check_targets(root: &Path) -> QualityResult<BTreeSet<String>> {
    let mut targets = BTreeSet::new();
    for relative in ["Makefile", "crates/terlan/cli.mk"] {
        let text = read_text(root, relative)?;
        for line in text.lines() {
            let Some((target, rest)) = line.split_once(':') else {
                continue;
            };
            if rest.starts_with('=') || target.starts_with('\t') || target.contains(' ') {
                continue;
            }
            if target.ends_with("-check") {
                targets.insert(target.to_string());
            }
        }
    }
    Ok(targets)
}

fn validate_artifacts(
    root: &Path,
    artifacts: &[ArtifactRow],
    inventory: &[InventoryRow],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let current_inventory = inventory
        .iter()
        .filter(|row| row.status == "current")
        .map(|row| row.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut artifact_paths = BTreeSet::new();
    let mut current_scopes = BTreeSet::new();
    for artifact in artifacts {
        if !artifact_paths.insert(artifact.path.as_str()) {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` contains duplicate artifact `{}`",
                artifact.path
            ));
        }
        if !["current", "nondeterministic"].contains(&artifact.status.as_str()) {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` artifact `{}` has unsupported execution status `{}`",
                artifact.path, artifact.status
            ));
        }
        if artifact.status == "nondeterministic"
            && (artifact.remediation_plan.trim().is_empty() || artifact.remediation_plan == "none")
        {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` nondeterministic artifact `{}` requires a remediation plan",
                artifact.path
            ));
        }
        if artifact.status == "current" && artifact.remediation_plan != "none" {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` current artifact `{}` must use remediation plan `none`",
                artifact.path
            ));
        }
        if !VALID_THEOREM_SCOPES.contains(&artifact.theorem_scope.as_str()) {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` artifact `{}` has invalid theorem scope `{}`",
                artifact.path, artifact.theorem_scope
            ));
        }
        if artifact.status == "current" {
            current_scopes.insert(artifact.theorem_scope.as_str());
        }
        if artifact.targeted_manifests.is_empty() {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` artifact `{}` has no targeted manifests",
                artifact.path
            ));
        }
        for manifest in &artifact.targeted_manifests {
            if !root.join(manifest).is_file() {
                diagnostics.push(format!(
                    "`{ARTIFACT_PATH}` artifact `{}` targets missing manifest `{manifest}`",
                    artifact.path
                ));
            }
        }
        if artifact.stderr_class != "none" {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` artifact `{}` has unsupported stderr class `{}`",
                artifact.path, artifact.stderr_class
            ));
        }
        let proof_path = root.join(&artifact.path);
        if !proof_path.is_file() {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` artifact `{}` references a missing proof",
                artifact.path
            ));
        } else {
            match sha256_file(&proof_path) {
                Ok(digest) if artifact.proof_digest != digest => diagnostics.push(format!(
                    "proof_gap[artifact-drift]: `{ARTIFACT_PATH}` artifact `{}` digest is stale: expected `{digest}`, found `{}`; update replay metadata after reproducibility passes or classify a blocker",
                    artifact.path, artifact.proof_digest
                )),
                Err(err) => diagnostics.push(err),
                _ => {}
            }
        }
        if !root.join(&artifact.replay_metadata).is_file() {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` artifact `{}` references missing replay metadata `{}`",
                artifact.path, artifact.replay_metadata
            ));
        }
        if !current_inventory.contains(artifact.path.as_str()) {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` artifact `{}` is not a current proof inventory row",
                artifact.path
            ));
        }
    }
    for path in current_inventory {
        if !artifact_paths.contains(path) {
            diagnostics.push(format!(
                "current proof inventory row `{path}` has no executable `{ARTIFACT_PATH}` entry"
            ));
        }
    }
    if artifacts.is_empty() {
        diagnostics.push(format!(
            "`{ARTIFACT_PATH}` must contain at least one executable proof artifact"
        ));
    }
    for required_scope in REQUIRED_CURRENT_THEOREM_SCOPES {
        if !current_scopes.contains(required_scope) {
            diagnostics.push(format!(
                "`{ARTIFACT_PATH}` requires a current executable `{required_scope}` proof family"
            ));
        }
    }
    diagnostics
}

fn sha256_file(path: &Path) -> QualityResult<String> {
    let bytes = fs::read(path).map_err(|err| {
        format!(
            "failed to read proof `{}` for digest: {err}",
            path.display()
        )
    })?;
    let digest = Sha256::digest(bytes);
    let hexadecimal = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("sha256:{hexadecimal}"))
}

fn validate_lean_files(root: &Path, lean_files: &[PathBuf]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for path in lean_files {
        let relative = normalize_path(path);
        let full_path = root.join(path);
        match fs::read_to_string(&full_path) {
            Ok(text) => diagnostics.extend(validate_stale_terms(&relative, &text)),
            Err(err) => diagnostics.push(format!("failed to read `{relative}`: {err}")),
        }
    }
    diagnostics
}

fn validate_stale_terms(path: &str, text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for term in STALE_TERMS {
        if text.contains(term) {
            diagnostics.push(format!(
                "`{path}` contains stale removed-runtime proof term `{term}`"
            ));
        }
    }
    diagnostics
}

fn collect_lean_files(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let proof_root = root.join(PROOF_ROOT);
    if !proof_root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_lean_files_inner(root, &proof_root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_lean_files_inner(
    root: &Path,
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> QualityResult<()> {
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read `{}`: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read directory entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_lean_files_inner(root, &path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("lean") {
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("failed to normalize `{}`: {err}", path.display()))?;
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|err| format!("failed to read `{relative}`: {err}"))
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("Lean proof track check failed:\n");
    for diagnostic in diagnostics {
        message.push_str("- ");
        message.push_str(diagnostic);
        message.push('\n');
    }
    message
}

#[cfg(test)]
#[path = "lean_proof_track_test.rs"]
#[cfg(test)]
mod lean_proof_track_test;
