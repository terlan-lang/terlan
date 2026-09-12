//! Supplement Git and resolver observations with resolved package source contents.

use super::{failure, resolver_cache};
use crate::source_inventory::SourceSnapshot;
use crate::PhaseFailure;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

/// Package roots and declared target files, including ignored, vendored and external sources.
pub(super) struct Binding {
    declarations: BTreeMap<PathBuf, PathBuf>,
    declarations_identity: String,
    roots: Vec<PathBuf>,
    covered: BTreeSet<PathBuf>,
    excluded: BTreeSet<PathBuf>,
    before: resolver_cache::Snapshot,
    after: Option<resolver_cache::Snapshot>,
}

impl Binding {
    /// Captures only supplemental bytes while referencing existing Git and resolver owners.
    pub(super) fn capture(
        document: &Value,
        root: &Path,
        source: &SourceSnapshot,
        cache: &resolver_cache::Binding,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let started = Instant::now();
        control.check(started).map_err(crate::process_failure)?;
        let target = absolute(&document["target_directory"])?;
        let excluded = BTreeSet::from([target, root.join(".terlan")]);
        let mut declarations = BTreeMap::new();
        let mut roots = BTreeSet::new();
        let packages = document["packages"]
            .as_array()
            .filter(|rows| !rows.is_empty() && rows.len() <= 16_384)
            .ok_or_else(|| failure("invalid resolved package source inventory"))?;
        for package in packages {
            control.check(started).map_err(crate::process_failure)?;
            let manifest = absolute(&package["manifest_path"])?;
            if manifest.file_name().is_none_or(|name| name != "Cargo.toml") {
                return Err(failure("resolved package has no Cargo.toml manifest"));
            }
            let manifest = resolve(&manifest, &mut declarations)?;
            roots.insert(
                manifest
                    .parent()
                    .ok_or_else(|| failure("manifest has no package root"))?
                    .to_owned(),
            );
            let targets = package["targets"]
                .as_array()
                .filter(|rows| rows.len() <= 4096)
                .ok_or_else(|| failure("missing resolved package target inventory"))?;
            for target in targets {
                control.check(started).map_err(crate::process_failure)?;
                let source = resolve(&absolute(&target["src_path"])?, &mut declarations)?;
                if source.components().any(|part| part.as_os_str() == ".git") {
                    return Err(failure(
                        "Rust target source is inside Git administrative storage",
                    ));
                }
                roots.insert(source);
            }
        }
        let mut selected: Vec<PathBuf> = Vec::new();
        for path in roots {
            control.check(started).map_err(crate::process_failure)?;
            if excluded.iter().any(|excluded| path.starts_with(excluded)) {
                return Err(failure(
                    "resolved package source is inside disposable build storage",
                ));
            }
            if !selected.iter().any(|parent| path.starts_with(parent)) {
                selected.push(path);
            }
        }
        let covered = source
            .covered_paths
            .union(cache.covered_paths())
            .cloned()
            .collect();
        let before = resolver_cache::observe_sources(&selected, &covered, &excluded, control)?;
        let mut identity = Sha256::new();
        for (declared, resolved) in &declarations {
            crate::file_identity::field(&mut identity, declared.as_os_str().as_encoded_bytes());
            crate::file_identity::field(&mut identity, resolved.as_os_str().as_encoded_bytes());
        }
        Ok(Self {
            declarations_identity: crate::file_identity::hex(identity),
            declarations,
            roots: selected,
            covered,
            excluded,
            before,
            after: None,
        })
    }

    /// Rechecks declared path resolution and supplemental contents at suite closeout.
    pub(super) fn verify(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        let started = Instant::now();
        control.check(started).map_err(crate::process_failure)?;
        if self.after.is_some() {
            return Err(failure("package source closeout cannot repeat"));
        }
        for (declared, resolved) in &self.declarations {
            control.check(started).map_err(crate::process_failure)?;
            if std::fs::canonicalize(declared).map_err(failure)? != *resolved {
                return Err(failure(
                    "resolved package source path changed during execution",
                ));
            }
        }
        self.after = Some(resolver_cache::observe_sources(
            &self.roots,
            &self.covered,
            &self.excluded,
            control,
        )?);
        if self.after.as_ref() != Some(&self.before) {
            return Err(failure("resolved package sources changed during execution"));
        }
        Ok(())
    }

    /// Reports this bounded source scope without claiming arbitrary build-script input closure.
    pub(super) fn json(&self) -> Value {
        json!({"scope":"resolved-package-supplemental-sources-v1", "roots":self.roots.len(),
            "declarations":self.declarations.len(), "declarations_sha256":self.declarations_identity,
            "referenced_input_files":self.covered.len(), "before":self.before.json(),
            "after":self.after.as_ref().map(resolver_cache::Snapshot::json),
            "verified":self.after.as_ref() == Some(&self.before), "reusable":false})
    }
}

fn absolute(value: &Value) -> Result<PathBuf, PhaseFailure> {
    let path = value
        .as_str()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| failure("package source path must be absolute"))?;
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(failure("package source path is not normalized"));
    }
    Ok(path)
}

fn resolve(
    path: &Path,
    declarations: &mut BTreeMap<PathBuf, PathBuf>,
) -> Result<PathBuf, PhaseFailure> {
    if let Some(resolved) = declarations.get(path) {
        return Ok(resolved.clone());
    }
    if declarations.len() == 65_536 {
        return Err(failure("excessive resolved package source declarations"));
    }
    let resolved = std::fs::canonicalize(path).map_err(failure)?;
    if !resolved.is_file() {
        return Err(failure("package source declaration is not a regular file"));
    }
    declarations.insert(path.to_owned(), resolved.clone());
    Ok(resolved)
}

#[cfg(test)]
#[path = "package_sources_test.rs"]
mod tests;
