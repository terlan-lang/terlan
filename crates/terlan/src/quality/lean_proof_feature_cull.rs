use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::terlan_quality::{render_failure, QualityResult};

const MAP_PATH: &str = "proofs/lean/feature_cull/removed_features.json";
const INVENTORY_PATH: &str = "docs/compiler/proof_track/lean_proof_inventory.tsv";
const ARTIFACTS_PATH: &str = "proofs/lean/ci/lean-proof-artifacts.tsv";
const ACTIVE_MANIFESTS: &[&str] = &[
    "docs/compiler/proof_track/lean_proof_gaps.tsv",
    "docs/compiler/type_spec/binary_descriptor_matrix.json",
    "docs/compiler/type_spec/language_feature_coverage_matrix.json",
    "docs/compiler/type_spec/operator_coverage_matrix.json",
    "docs/compiler/type_spec/pattern_matching_support_matrix.json",
    ARTIFACTS_PATH,
];
const REQUIRED_FEATURES: &[&str] = &[
    "beam_lowering",
    "core_v0_profile",
    "legacy_import_namespace",
    "legacy_test_runtime",
    "legacy_tuple_destructuring_default",
    "native_bridge",
    "vm_profile",
];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Counts the retired features and aliases enforced by the proof-cull gate.
pub struct LeanProofFeatureCullSummary {
    pub removed_feature_count: usize,
    pub rejection_theorem_count: usize,
    pub forbidden_alias_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Machine-readable retirement map tying features to proofs and replacement gates.
struct FeatureCullMap {
    schema: String,
    terlan_version: String,
    proof: String,
    proof_artifact: String,
    features: Vec<RemovedFeature>,
    forbidden_make_targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// One removed feature and the evidence preventing its reintroduction.
struct RemovedFeature {
    id: String,
    rejection_phase: String,
    replacement_contract: String,
    replacement_gate: String,
    theorem: String,
    forbidden_terms: Vec<String>,
}

/// Validates the one-way formal boundary around removed 0.0.7 constructs.
pub fn run_lean_proof_feature_cull(root: &Path) -> QualityResult<LeanProofFeatureCullSummary> {
    let map_text = read_text(root, MAP_PATH)?;
    let map = serde_json::from_str::<FeatureCullMap>(&map_text)
        .map_err(|err| format!("`{MAP_PATH}` is not valid feature-cull JSON: {err}"))?;
    let diagnostics = validate_feature_cull(root, &map);
    if !diagnostics.is_empty() {
        return Err(render_failure("lean-proof-feature-cull", &diagnostics));
    }
    Ok(LeanProofFeatureCullSummary {
        removed_feature_count: map.features.len(),
        rejection_theorem_count: map.features.len(),
        forbidden_alias_count: map.forbidden_make_targets.len(),
    })
}

/// Validates the complete feature-cull map against source and proof artifacts.
fn validate_feature_cull(root: &Path, map: &FeatureCullMap) -> Vec<String> {
    let mut diagnostics = validate_map_header(map);
    let make_text = read_make_graph(root, &mut diagnostics);
    let make_targets = collect_make_targets(&make_text);
    let proof_text = read_for_validation(root, &map.proof, &mut diagnostics);
    let artifact_text = read_for_validation(root, &map.proof_artifact, &mut diagnostics);
    let inventory = read_for_validation(root, INVENTORY_PATH, &mut diagnostics);
    let artifacts = read_for_validation(root, ARTIFACTS_PATH, &mut diagnostics);

    diagnostics.extend(validate_features(
        map,
        &make_targets,
        &proof_text,
        &artifact_text,
    ));
    diagnostics.extend(validate_proof_registration(map, &inventory, &artifacts));
    diagnostics.extend(validate_forbidden_aliases(map, &make_targets));
    diagnostics.extend(validate_active_manifests(root, map));
    diagnostics.extend(validate_other_lean_files(root, map));
    diagnostics
}

/// Validates schema, release version, feature completeness, and ordering.
fn validate_map_header(map: &FeatureCullMap) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if map.schema != "terlan.lean-feature-cull.v1" {
        diagnostics.push(format!("unsupported feature-cull schema `{}`", map.schema));
    }
    if map.terlan_version != "0.0.7" {
        diagnostics.push(format!(
            "feature-cull map targets `{}`, expected `0.0.7`",
            map.terlan_version
        ));
    }
    let ids = map
        .features
        .iter()
        .map(|feature| feature.id.as_str())
        .collect::<Vec<_>>();
    if ids != REQUIRED_FEATURES {
        diagnostics.push(format!(
            "feature-cull IDs must be complete and sorted: expected {REQUIRED_FEATURES:?}, found {ids:?}"
        ));
    }
    if !strictly_sorted(&map.forbidden_make_targets) {
        diagnostics.push("forbidden Make targets must be unique and sorted".to_string());
    }
    diagnostics
}

/// Validates each removed feature's gate, theorem, artifact, and forbidden terms.
fn validate_features(
    map: &FeatureCullMap,
    make_targets: &BTreeSet<String>,
    proof: &str,
    artifact: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut theorems = BTreeSet::new();
    for feature in &map.features {
        if !["parse", "typecheck", "target-selection", "compile"]
            .contains(&feature.rejection_phase.as_str())
        {
            diagnostics.push(format!(
                "feature `{}` has invalid rejection phase `{}`",
                feature.id, feature.rejection_phase
            ));
        }
        if feature.replacement_contract.trim().is_empty() {
            diagnostics.push(format!(
                "feature `{}` has no replacement contract",
                feature.id
            ));
        }
        if !make_targets.contains(&feature.replacement_gate) {
            diagnostics.push(format!(
                "feature `{}` replacement gate `{}` is not a Make target",
                feature.id, feature.replacement_gate
            ));
        }
        if !theorems.insert(feature.theorem.as_str()) {
            diagnostics.push(format!("duplicate rejection theorem `{}`", feature.theorem));
        }
        let theorem_name = feature.theorem.rsplit('.').next().unwrap_or_default();
        if !proof.contains(&format!("theorem {theorem_name}")) {
            diagnostics.push(format!(
                "feature `{}` theorem `{}` is missing from `{}`",
                feature.id, feature.theorem, map.proof
            ));
        }
        if !artifact.contains(&format!("\"{}\"", feature.theorem)) {
            diagnostics.push(format!(
                "feature `{}` theorem `{}` is missing from `{}`",
                feature.id, feature.theorem, map.proof_artifact
            ));
        }
        if feature.forbidden_terms.is_empty() || !strictly_sorted(&feature.forbidden_terms) {
            diagnostics.push(format!(
                "feature `{}` forbidden terms must be nonempty, unique, and sorted",
                feature.id
            ));
        }
    }
    for acceptance in [
        "everyRetiredAssumptionIsBlockedBeforeVm",
        "noProofArtifactUsesRetiredAssumptions",
    ] {
        if !proof.contains(&format!("theorem {acceptance}")) {
            diagnostics.push(format!("acceptance theorem `{acceptance}` is missing"));
        }
    }
    diagnostics
}

/// Verifies the rejection proof is registered in both proof inventories.
fn validate_proof_registration(
    map: &FeatureCullMap,
    inventory: &str,
    artifacts: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if !inventory
        .lines()
        .any(|line| line.starts_with(&format!("{}\tcurrent\t", map.proof)))
    {
        diagnostics.push(format!(
            "`{}` is not a current proof inventory row",
            map.proof
        ));
    }
    if !artifacts.lines().any(|line| {
        line.starts_with(&format!("{}\tcurrent\trejection\t", map.proof))
            && line.contains(&map.proof_artifact)
    }) {
        diagnostics.push(format!(
            "`{}` is not registered as a current rejection artifact",
            map.proof
        ));
    }
    diagnostics
}

/// Reports retired Make target aliases that have reappeared.
fn validate_forbidden_aliases(
    map: &FeatureCullMap,
    make_targets: &BTreeSet<String>,
) -> Vec<String> {
    map.forbidden_make_targets
        .iter()
        .filter(|target| make_targets.contains(target.as_str()))
        .map(|target| format!("removed fallback Make target `{target}` has been restored"))
        .collect()
}

/// Scans active proof manifests and artifacts for retired feature terminology.
fn validate_active_manifests(root: &Path, map: &FeatureCullMap) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for relative in ACTIVE_MANIFESTS {
        let text = read_for_validation(root, relative, &mut diagnostics);
        diagnostics.extend(stale_term_diagnostics(relative, &text, map));
    }
    let artifact_root = root.join("proofs/lean/artifacts");
    if let Ok(entries) = fs::read_dir(&artifact_root) {
        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            if path.extension().and_then(|extension| extension.to_str()) != Some("json")
                || path == root.join(&map.proof_artifact)
            {
                continue;
            }
            let relative = normalize_path(root, &path);
            let text = read_for_validation(root, &relative, &mut diagnostics);
            diagnostics.extend(stale_term_diagnostics(&relative, &text, map));
        }
    }
    diagnostics
}

/// Scans Lean sources outside the canonical rejection proof for stale terms.
fn validate_other_lean_files(root: &Path, map: &FeatureCullMap) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut pending = vec![root.join("proofs/lean")];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("lean") {
                let relative = normalize_path(root, &path);
                if relative == map.proof {
                    continue;
                }
                let text = read_for_validation(root, &relative, &mut diagnostics);
                diagnostics.extend(stale_term_diagnostics(&relative, &text, map));
            }
        }
    }
    diagnostics
}

/// Produces path-specific diagnostics for forbidden retired-feature terms.
fn stale_term_diagnostics(path: &str, text: &str, map: &FeatureCullMap) -> Vec<String> {
    map.features
        .iter()
        .flat_map(|feature| {
            feature.forbidden_terms.iter().filter_map(move |term| {
                text.contains(term).then(|| {
                    format!(
                        "`{path}` restores removed feature `{}` through term `{term}`",
                        feature.id
                    )
                })
            })
        })
        .collect()
}

/// Reads Make fragments used to resolve replacement and forbidden targets.
fn read_make_graph(root: &Path, diagnostics: &mut Vec<String>) -> String {
    ["Makefile", "crates/terlan/cli.mk"]
        .iter()
        .map(|relative| read_for_validation(root, relative, diagnostics))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collects concrete target names from the release Make graph.
fn collect_make_targets(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter_map(|line| {
            let (target, rest) = line.split_once(':')?;
            (!target.is_empty() && !target.contains([' ', '\t', '$']) && !rest.starts_with('='))
                .then(|| target.to_string())
        })
        .collect()
}

/// Reports whether values are unique and strictly lexicographically ordered.
fn strictly_sorted(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// Reads validation input while converting I/O failures into gate diagnostics.
fn read_for_validation(root: &Path, relative: &str, diagnostics: &mut Vec<String>) -> String {
    match read_text(root, relative) {
        Ok(text) => text,
        Err(error) => {
            diagnostics.push(error);
            String::new()
        }
    }
}

/// Reads one repository-relative UTF-8 proof or manifest file.
fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|err| format!("failed to read `{relative}`: {err}"))
}

/// Produces a stable slash-separated repository-relative path.
fn normalize_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
#[path = "lean_proof_feature_cull_test.rs"]
mod lean_proof_feature_cull_test;
