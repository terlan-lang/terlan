//! Admit a declared Cargo test harness before issuing any libtest query.

use crate::{file_identity, PhaseFailure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

/// One bounded manifest parse per package in a Cargo artifact observation cycle.
pub(super) struct ManifestAdmission {
    root: PathBuf,
    manifests: BTreeMap<PathBuf, ManifestSnapshot>,
    bytes: usize,
}

struct ManifestSnapshot {
    document: toml::Value,
    identity: String,
}

impl ManifestAdmission {
    /// Freezes the source root before collecting any Cargo artifacts.
    pub(super) fn new(root: &Path) -> Result<Self, PhaseFailure> {
        Ok(Self {
            root: fs::canonicalize(root).map_err(failure)?,
            manifests: BTreeMap::new(),
            bytes: 0,
        })
    }

    /// Reuses a package snapshot, while each declaration retains independent closeout checks.
    pub(super) fn admit(
        &mut self,
        artifact: &Value,
        control: ProcessControl<'_>,
    ) -> Result<DeclaredHarness, PhaseFailure> {
        control
            .check(Instant::now())
            .map_err(crate::process_failure)?;
        let manifest = artifact_path(artifact, "manifest_path")?;
        if manifest.file_name() != Some(std::ffi::OsStr::new("Cargo.toml")) {
            return Err(failure("Cargo test artifact has no package manifest"));
        }
        if !fs::symlink_metadata(&manifest).map_err(failure)?.is_file() {
            return Err(failure("Cargo manifest is not a regular file"));
        }
        let resolved = contained(&self.root, &manifest)?;
        if !self.manifests.contains_key(&resolved) {
            if self.manifests.len() >= 256 {
                return Err(failure("Cargo manifest snapshot count exceeds its budget"));
            }
            let (bytes, identity) = manifest_bytes(&manifest, control)?;
            if bytes.len() > (16 * 1024 * 1024_usize).saturating_sub(self.bytes) {
                return Err(failure("Cargo manifest snapshots exceed their byte budget"));
            }
            let document =
                toml::from_str(std::str::from_utf8(&bytes).map_err(failure)?).map_err(failure)?;
            self.bytes += bytes.len();
            self.manifests
                .insert(resolved.clone(), ManifestSnapshot { document, identity });
        }
        let snapshot = &self.manifests[&resolved];
        admit_snapshot(artifact, &self.root, manifest, snapshot)
    }

    /// Re-admits a retained target through the same shared current manifest snapshots.
    pub(super) fn restore(
        &mut self,
        evidence: &Value,
        control: ProcessControl<'_>,
    ) -> Result<DeclaredHarness, PhaseFailure> {
        if evidence["scope"] != "declared-cargo-libtest-target-v1" {
            return Err(failure("saved target has no declared libtest scope"));
        }
        let artifact = serde_json::json!({"reason":"compiler-artifact", "profile":{"test":true},
            "executable":evidence["executable"],"manifest_path":evidence["manifest"],
            "target":{"kind":[evidence["kind"].clone()],"name":evidence["target"],"src_path":evidence["source"]}});
        let declaration = self.admit(&artifact, control)?;
        if declaration.json() != *evidence {
            return Err(failure(
                "saved harness declaration differs from current inputs",
            ));
        }
        Ok(declaration)
    }
}

/// Source declaration and selected executable for a Cargo libtest target.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeclaredHarness {
    /// Exact Cargo-selected executable; byte admission remains with ExecutableBinding.
    pub(super) executable: PathBuf,
    root: PathBuf,
    manifest: PathBuf,
    manifest_identity: String,
    source: PathBuf,
    resolved_source: PathBuf,
    explicit_harness: bool,
    package: String,
    name: String,
    kind: String,
}

impl DeclaredHarness {
    /// Re-admits a saved library declaration without launching Cargo or the harness.
    pub(super) fn restore_library(
        evidence: &Value,
        root: &Path,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        if evidence["scope"] != "declared-cargo-libtest-target-v1"
            || evidence["package"] != "terlan"
            || evidence["kind"] != "lib"
        {
            return Err(failure("saved harness is not the canonical library target"));
        }
        ManifestAdmission::new(root)?.restore(evidence, control)
    }

    /// Requires the package working directory Cargo selected for this target.
    pub(super) fn verify_directory(&self, directory: &Path) -> Result<(), PhaseFailure> {
        let parent = self
            .manifest
            .parent()
            .ok_or_else(|| failure("missing package directory"))?;
        if fs::canonicalize(parent).map_err(failure)?
            != fs::canonicalize(directory).map_err(failure)?
        {
            return Err(failure(
                "Cargo runner package directory does not match its declaration",
            ));
        }
        Ok(())
    }

    /// Rejects manifest mutation or source alias retargeting before another query/run.
    pub(super) fn verify(&self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        contained(&self.root, &self.manifest)?;
        if manifest_bytes(&self.manifest, control)?.1 != self.manifest_identity
            || contained(&self.root, &self.source)? != self.resolved_source
        {
            return Err(failure("Cargo harness declaration changed after admission"));
        }
        Ok(())
    }

    /// Does not claim that manifest declaration proves arbitrary compiler output is libtest.
    pub(super) fn json(&self) -> Value {
        serde_json::json!({"scope": "declared-cargo-libtest-target-v1", "package": self.package, "target": self.name, "kind": self.kind, "manifest": self.manifest,
            "manifest_identity_sha256": self.manifest_identity, "source": self.source,
            "resolved_source": self.resolved_source, "executable": self.executable,
            "harness": true, "harness_setting": if self.explicit_harness { "explicit" } else { "cargo-default" }})
    }
}

/// Requires exactly one artifact and a source-bound libtest declaration, not profile.test alone.
#[cfg(test)]
pub(super) fn select(
    output: &[u8],
    root: &Path,
    control: ProcessControl<'_>,
) -> Result<DeclaredHarness, PhaseFailure> {
    let mut stream = crate::cargo_artifact_stream::CargoArtifactStream::terlan_library(root)?;
    stream.observe(output, control, |_| Ok(()))?;
    stream.finish_terlan_library()
}

/// Admits a library, binary, or integration-test artifact before any executable probe.
#[cfg(test)]
pub(super) fn admit(
    artifact: &Value,
    root: &Path,
    control: ProcessControl<'_>,
) -> Result<DeclaredHarness, PhaseFailure> {
    ManifestAdmission::new(root)?.admit(artifact, control)
}

fn admit_snapshot(
    artifact: &Value,
    root: &Path,
    manifest: PathBuf,
    snapshot: &ManifestSnapshot,
) -> Result<DeclaredHarness, PhaseFailure> {
    if artifact["reason"] != "compiler-artifact" || artifact["profile"]["test"] != true {
        return Err(failure("Cargo artifact is not a test target"));
    }
    let executable = artifact_path(artifact, "executable")?;
    if !executable.is_file() {
        return Err(PhaseFailure {
            outcome: "artifact-missing",
            detail: format!(
                "Cargo selected missing Terlan test harness {}",
                executable.display()
            ),
        });
    }
    let declared = crate::cargo_target_declaration::resolve(
        &snapshot.document,
        &artifact["target"],
        manifest
            .parent()
            .ok_or_else(|| failure("Cargo manifest has no parent"))?,
    )?;
    let source = declared.source;
    let resolved_source = contained(root, &source)?;
    let artifact_source = artifact_path(&artifact["target"], "src_path")?;
    if contained(root, &artifact_source)? != resolved_source || !resolved_source.is_file() {
        return Err(failure("Cargo artifact and declared test source disagree"));
    }
    Ok(DeclaredHarness {
        executable,
        root: root.to_path_buf(),
        manifest,
        manifest_identity: snapshot.identity.clone(),
        source,
        resolved_source,
        explicit_harness: declared.explicit_harness,
        package: declared.package,
        name: declared.name,
        kind: declared.kind.to_owned(),
    })
}

fn artifact_path(artifact: &Value, key: &str) -> Result<PathBuf, PhaseFailure> {
    let path = artifact[key]
        .as_str()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| failure(format!("Cargo artifact has no absolute {key}")))?;
    Ok(path)
}

fn contained(root: &Path, path: &Path) -> Result<PathBuf, PhaseFailure> {
    let resolved = fs::canonicalize(path).map_err(failure)?;
    if !resolved.starts_with(root) {
        return Err(failure(
            "Cargo harness declaration escapes the admitted source root",
        ));
    }
    Ok(resolved)
}

fn manifest_bytes(
    path: &Path,
    control: ProcessControl<'_>,
) -> Result<(Vec<u8>, String), PhaseFailure> {
    let mut identity = Sha256::new();
    let bytes =
        file_identity::read_hashed_file(path, &mut identity, 1024 * 1024, control, Instant::now())?;
    Ok((bytes, file_identity::hex(identity)))
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "harness-declaration-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_harness_admission_test.rs"]
mod tests;
