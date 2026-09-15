//! Selected proof consumers share the track's isolated, validated replica owner.

use super::*;

/// Verifies exact current proof paths without publishing a partial track report.
///
/// Repeated callers reuse the same input/tool-bound replicas on Linux. Other
/// platforms retain isolated execution rather than trusting unchecked receipts.
pub(crate) fn run(root: &Path, paths: &[String]) -> QualityResult<usize> {
    Ok(lean_proof_repro::run_replicas(root, &admitted(root, paths)?)?.len())
}

/// A preparation consumer must not silently become a producer on a cache miss.
pub(crate) fn require_completed(root: &Path, paths: &[String]) -> QualityResult<usize> {
    Ok(lean_proof_repro::require_replicas(root, &admitted(root, paths)?)?.len())
}

fn admitted(root: &Path, paths: &[String]) -> QualityResult<Vec<ArtifactRow>> {
    let artifacts = parse_artifacts(&read_text(root, ARTIFACT_PATH)?)?;
    let inventory = parse_inventory(&read_text(root, INVENTORY_PATH)?)?;
    let selected = select(artifacts, paths)?;
    for artifact in &selected {
        if !inventory
            .iter()
            .any(|row| row.path == artifact.path && row.status == "current")
        {
            return Err(format!(
                "proof is not current in inventory: {}",
                artifact.path
            ));
        }
        if sha256_file(&root.join(&artifact.path))? != artifact.proof_digest {
            return Err(format!("proof_gap[artifact-drift]: {}", artifact.path));
        }
    }
    Ok(selected)
}

fn select(artifacts: Vec<ArtifactRow>, paths: &[String]) -> QualityResult<Vec<ArtifactRow>> {
    if paths.is_empty() {
        return Err("lean-proof-replay requires at least one exact proofs/lean/*.lean path".into());
    }
    let mut requested = BTreeSet::new();
    for path in paths {
        if !path.starts_with("proofs/lean/")
            || !path.ends_with(".lean")
            || path.contains('\\')
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(format!("invalid exact proof path: {path}"));
        }
        if !requested.insert(path.as_str()) {
            return Err(format!("duplicate proof request: {path}"));
        }
    }
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    for artifact in artifacts {
        if !seen.insert(artifact.path.clone()) {
            return Err(format!("duplicate proof artifact: {}", artifact.path));
        }
        if requested.remove(artifact.path.as_str()) {
            if artifact.status != "current"
                || artifact.expected_exit != 0
                || artifact.remediation_plan != "none"
            {
                return Err(format!(
                    "proof is not admitted for successful replay: {}",
                    artifact.path
                ));
            }
            selected.push(artifact);
        }
    }
    if !requested.is_empty() {
        return Err(format!("unknown proof requests: {requested:?}"));
    }
    Ok(selected)
}

#[cfg(test)]
#[path = "lean_proof_selection_test.rs"]
mod tests;
