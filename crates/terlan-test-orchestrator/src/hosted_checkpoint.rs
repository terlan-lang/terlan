//! Admit an owned authenticated-download checkpoint, not a caller's skip marker.

use crate::coverage_requests::failure;
use crate::file_identity::{hash_file_contents, hex, read_hashed_file};
use crate::hosted_coverage::{self, Bundle};
use crate::test_selections::read_document;
use crate::PhaseFailure;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

/// Reads the selected coverage subjects while excluding download/retention writers.
/// The immutable in-memory result outlives that lease; it never reopens the cache.
pub(super) fn admit(
    root: &Path,
    revision: &str,
    control: ProcessControl<'_>,
) -> Result<(Bundle, Value), PhaseFailure> {
    let _lease = crate::report_file::read_lease(&root.join("target/publication-inputs"))
        .map_err(failure)?
        .ok_or_else(|| failure("missing publication input owner"))?;
    directory(&root.join("target"))?;
    directory(&root.join("target/quality"))?;
    directory(&root.join("target/publication-downloads"))?;
    let (candidate, candidate_digest) = read_document(
        &root.join("target/quality/hosted-candidate-validation.json"),
        1024 * 1024,
        control,
    )?;
    let key = candidate["coverage"]["cache_key"]
        .as_str()
        .filter(|key| digest(key))
        .ok_or_else(|| failure("candidate lacks a verified coverage cache key"))?;
    let cache = root.join("target/publication-downloads").join(key);
    directory(&cache)?;
    directory(&cache.join("evidence"))?;
    let (context, context_digest) = read_document(&cache.join("context.json"), 16 * 1024, control)?;
    let mut implementation = Sha256::new();
    hash_file_contents(
        &root.join("scripts/download_validated_release_artifacts.sh"),
        &mut implementation,
        1024 * 1024,
        control,
        Instant::now(),
    )?;
    if context_digest != key
        || context["revision"] != revision
        || candidate["schema"] != "terlan.hosted-candidate-validation.v1"
        || candidate["decision"] != "pass"
        || candidate["source_revision"] != revision
        || candidate["workflow"] != ".github/workflows/ci.yml"
        || candidate["run_id"] != context["compiler"]["id"]
        || candidate["coverage"]["authentication"] != "github-attestation"
        || candidate["coverage"]["producer_job_id"] != context["compiler"]["job_id"]
        || context["compiler"]["job_id"]
            .as_u64()
            .is_none_or(|id| id == 0)
        || candidate["coverage"]["producer_attempt"] != context["compiler"]["attempt"]
        || context["implementation"] != hex(implementation)
    {
        return Err(failure(
            "hosted checkpoint origin, source or verifier implementation changed",
        ));
    }
    let bytes = read_hashed_file(
        &cache.join("verified-files.sha256"),
        &mut Sha256::new(),
        1024 * 1024,
        control,
        Instant::now(),
    )?;
    let manifest = manifest(&bytes)?;
    let (cached_candidate, cached_digest) = read_document(
        &cache.join("evidence/hosted-candidate-validation.json"),
        1024 * 1024,
        control,
    )?;
    if candidate != cached_candidate || candidate_digest != cached_digest {
        return Err(failure(
            "installed candidate differs from its authenticated checkpoint",
        ));
    }
    required(&manifest, "context.json", &context_digest)?;
    required(
        &manifest,
        "evidence/hosted-candidate-validation.json",
        &candidate_digest,
    )?;
    let bundle = hosted_coverage::read(&cache.join("coverage"), &context, control)?;
    if candidate["coverage"]["records"] != bundle.summary {
        return Err(failure(
            "authenticated coverage subjects or reader contract changed",
        ));
    }
    for (name, hash) in bundle.summary["files"]
        .as_object()
        .expect("fixed coverage subjects")
    {
        required(
            &manifest,
            &format!("coverage/{name}"),
            hash.as_str()
                .ok_or_else(|| failure("missing subject digest"))?,
        )?;
    }
    let provenance = json!({"scope":"owned-authenticated-download-checkpoint-v1","cache_key":key,
        "candidate_sha256":candidate_digest,"producer":context["compiler"],
        "records":bundle.summary,"distribution_bytes_revalidated":false,
        "current_artifact_test_equivalence":false});
    Ok((bundle, provenance))
}

fn directory(path: &Path) -> Result<(), PhaseFailure> {
    if !std::fs::symlink_metadata(path).map_err(failure)?.is_dir() {
        return Err(failure("hosted checkpoint contains a redirected directory"));
    }
    Ok(())
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn manifest(bytes: &[u8]) -> Result<BTreeMap<String, String>, PhaseFailure> {
    let text = std::str::from_utf8(bytes).map_err(failure)?;
    if !text.ends_with('\n') {
        return Err(failure("incomplete hosted checkpoint manifest"));
    }
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let (hash, path) = line
            .split_once("  ./")
            .ok_or_else(|| failure("malformed checkpoint checksum"))?;
        if !digest(hash)
            || path.is_empty()
            || path.len() > 4096
            || path.contains('\\')
            || path.chars().any(char::is_control)
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
            || Path::new(path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || result.len() == 4096
            || result.insert(path.into(), hash.into()).is_some()
        {
            return Err(failure("invalid or duplicate checkpoint checksum entry"));
        }
    }
    Ok(result)
}

fn required(
    manifest: &BTreeMap<String, String>,
    path: &str,
    hash: &str,
) -> Result<(), PhaseFailure> {
    if manifest.get(path).map(String::as_str) != Some(hash) {
        return Err(failure(format!(
            "checkpoint does not bind coverage subject: {path}"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "hosted_checkpoint_test.rs"]
mod tests;
