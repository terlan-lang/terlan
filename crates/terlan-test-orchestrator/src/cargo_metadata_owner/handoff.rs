//! Suite admission of a completed metadata generation, not cross-cycle cache reuse.

use super::{failure, package_sources, resolver_cache};
use crate::execution_environment::{identity, ExecutionEnvironment};
use crate::file_identity::read_hashed_file;
use crate::validation_inputs::ValidationInputs;
use crate::PhaseFailure;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

/// Immutable package selection retained by the suite after input admission.
pub(crate) struct Handoff {
    packages: BTreeMap<String, PathBuf>,
    doctest_targets: Vec<crate::rustdoc_owner::Target>,
    summary: Value,
    cache: resolver_cache::Binding,
    sources: package_sources::Binding,
}

impl Handoff {
    /// Reads a complete generation and compares existing suite observations.
    pub(crate) fn admit(
        inputs: &ValidationInputs,
        environment: &ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let root = home::env::Env::current_dir(environment).map_err(failure)?;
        let root = std::fs::canonicalize(root).map_err(failure)?;
        let path = root.join("target/quality/rust-cargo-metadata.json");
        let attempt_path = path.with_extension("json.attempt.json");
        let attempt = read(&attempt_path, 1024 * 1024, control)?;
        let bytes = read(&path, 32 * 1024 * 1024, control)?;
        if read(&attempt_path, 1024 * 1024, control)? != attempt {
            return Err(failure(
                "metadata generation changed during suite admission",
            ));
        }
        let document: Value = serde_json::from_slice(&bytes).map_err(failure)?;
        let attempt: Value = serde_json::from_slice(&attempt).map_err(failure)?;
        let digest = crate::file_identity::hex(Sha256::new_with_prefix(&bytes));
        let observation = &document["terlan_preparation"];
        admit_generation(&document, &attempt, &root, &digest)?;
        for (key, current) in [
            ("source", inputs.source.json()),
            ("configuration", inputs.configuration.json()),
        ] {
            if observation[key]["before"] != current["before"] || current["before"].is_null() {
                return Err(failure(format!(
                    "metadata {key} differs from suite admission"
                )));
            }
        }
        let current_tools = inputs.executables.json();
        for role in ["cargo", "git", "rustup"] {
            if row(&observation["executables"], role) != row(&current_tools, role) {
                return Err(failure(format!(
                    "metadata executable selection differs: {role}"
                )));
            }
        }
        if observation["cargo_dispatch"]["kind"] == "rustup-proxy" {
            let native = inputs.toolchain.json();
            if row(
                &observation["cargo_dispatch"]["native_cargo"],
                "native-cargo",
            ) != row(&native["native_tools"], "native-cargo")
            {
                return Err(failure(
                    "metadata native Cargo differs from suite admission",
                ));
            }
        }
        if observation["environment_sha256"]
            != identity(&super::command(environment, Path::new("cargo")))
        {
            return Err(failure("metadata environment differs from suite admission"));
        }
        let cache = resolver_cache::Binding::capture(environment, control)?;
        if observation["resolver_cache"]["before"] != cache.json()["before"] {
            return Err(failure(
                "metadata resolver cache differs from suite admission",
            ));
        }
        let (packages, doctest_targets) = workspace_packages(&document, &root)?;
        let sources = package_sources::Binding::capture(
            &document,
            &root,
            inputs
                .source
                .before
                .as_ref()
                .ok_or_else(|| failure("package sources require Git source admission"))?,
            &cache,
            control,
        )?;
        let summary = json!({
            "scope":"same-input-cargo-metadata-handoff-v1", "reusable":false,
            "run_id":observation["run_id"], "metadata_sha256":digest,
            "workspace_packages":packages, "doctest_targets":doctest_targets, "admitted":true,
        });
        Ok(Self {
            packages,
            doctest_targets,
            summary,
            cache,
            sources,
        })
    }

    /// Allows only a package declared by the independently admitted metadata.
    pub(crate) fn admit_harness(
        packages: &BTreeMap<String, PathBuf>,
        declaration: &Value,
    ) -> Result<(), PhaseFailure> {
        let id = declaration["package"]
            .as_str()
            .ok_or_else(|| failure("harness has no package name"))?;
        let path = declaration["manifest"].as_str().map(Path::new);
        if packages.get(id).map(PathBuf::as_path) != path || path.is_none() {
            return Err(failure(
                "Cargo harness is not an admitted workspace package",
            ));
        }
        Ok(())
    }

    /// Shares the small admitted package index with independently owned phases.
    pub(crate) fn packages(&self) -> BTreeMap<String, PathBuf> {
        self.packages.clone()
    }

    /// Provides the bounded library selection independently of emitted Rustdoc output.
    pub(crate) fn doctest_targets(&self) -> Vec<crate::rustdoc_owner::Target> {
        self.doctest_targets.clone()
    }

    /// Resolver mutation invalidates this observation even if source still matches.
    pub(crate) fn verify(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        self.cache.verify(control)?;
        self.sources.verify(control)
    }

    /// Reports the selected generation without embedding the full dependency graph.
    pub(crate) fn json(&self) -> Value {
        let mut summary = self.summary.clone();
        summary["resolver_cache"] = self.cache.json();
        summary["package_sources"] = self.sources.json();
        summary
    }
}

fn row<'a>(binding: &'a Value, role: &str) -> Option<&'a Value> {
    binding["before"]
        .as_array()?
        .iter()
        .find(|row| row["role"] == role)
}

fn read(path: &Path, limit: u64, control: ProcessControl<'_>) -> Result<Vec<u8>, PhaseFailure> {
    read_hashed_file(path, &mut Sha256::new(), limit, control, Instant::now())
}

fn verified(observation: &Value, key: &str) -> bool {
    let value = &observation[key];
    value["verified"] == true && !value["before"].is_null() && value["before"] == value["after"]
}

fn admit_generation(
    document: &Value,
    attempt: &Value,
    root: &Path,
    digest: &str,
) -> Result<(), PhaseFailure> {
    let observation = &document["terlan_preparation"];
    if document["version"] != 1
        || document["workspace_root"].as_str().map(Path::new) != Some(root)
        || observation["schema"] != "terlan.cargo-metadata-observation.v1"
        || observation["reusable"] != false
        || observation["run_id"].as_str().is_none_or(str::is_empty)
        || attempt["schema"] != "terlan.cargo-metadata-attempt.v1"
        || attempt["state"] != "succeeded"
        || attempt["reusable"] != false
        || attempt["run_id"] != observation["run_id"]
        || attempt["metadata_sha256"] != digest
        || attempt["launches"] != observation["launches"]
        || observation["query"]
            != json!([
                "metadata",
                "--locked",
                "--all-features",
                "--format-version",
                "1"
            ])
        || !["source", "configuration", "executables", "resolver_cache"]
            .iter()
            .all(|key| verified(observation, key))
    {
        return Err(failure(
            "metadata handoff is incomplete, corrupt, or from another generation",
        ));
    }
    match observation["cargo_dispatch"]["kind"].as_str() {
        Some("rustup-proxy") if verified(&observation["cargo_dispatch"], "native_cargo") => Ok(()),
        Some("supplied-entry-point") => Ok(()),
        _ => Err(failure("metadata handoff has no admitted Cargo dispatch")),
    }
}

fn workspace_packages(
    document: &Value,
    root: &Path,
) -> Result<(BTreeMap<String, PathBuf>, Vec<crate::rustdoc_owner::Target>), PhaseFailure> {
    super::validate_packages(document)?;
    let mut packages = BTreeMap::new();
    let mut selected = Vec::new();
    for member in document["workspace_members"]
        .as_array()
        .ok_or_else(|| failure("missing workspace members"))?
    {
        let id = member
            .as_str()
            .ok_or_else(|| failure("invalid member ID"))?;
        let package = document["packages"]
            .as_array()
            .and_then(|packages| packages.iter().find(|p| p["id"] == id))
            .ok_or_else(|| failure("missing workspace package"))?;
        selected.push(package);
        let manifest = package["manifest_path"]
            .as_str()
            .map(PathBuf::from)
            .filter(|path| {
                path.is_absolute()
                    && path.starts_with(root)
                    && path.file_name().is_some_and(|name| name == "Cargo.toml")
            })
            .ok_or_else(|| failure("workspace package has an invalid manifest path"))?;
        let name = package["name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| failure("workspace package has no name"))?;
        if manifest.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        }) || packages.insert(name.into(), manifest).is_some()
        {
            return Err(failure(
                "workspace package names or manifest paths are ambiguous",
            ));
        }
    }
    Ok((packages, crate::rustdoc_owner::project(&selected, root)?))
}

#[cfg(test)]
#[path = "handoff_test.rs"]
mod tests;
