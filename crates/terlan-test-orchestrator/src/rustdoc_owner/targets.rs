//! Cargo-resolved workspace doctest targets, independent of Rustdoc output.

use super::failure;
use crate::PhaseFailure;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// One Cargo library which must receive exactly one Rustdoc test invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Target {
    /// Cargo package name selected by the admitted workspace membership.
    pub(crate) package: String,
    /// Rustdoc's normalized crate-name argument.
    pub(crate) crate_name: String,
    /// Canonical owning package manifest.
    pub(crate) manifest: PathBuf,
    /// Canonical library source passed to Rustdoc.
    pub(crate) source: PathBuf,
    /// Edition which determines Rustdoc compilation and isolation behavior.
    pub(crate) edition: String,
}

impl Target {
    /// Stable namespace key without interpreting a package name as a filename.
    pub(crate) fn key(&self) -> Result<String, PhaseFailure> {
        Ok(crate::file_identity::hex(Sha256::new_with_prefix(
            serde_json::to_vec(self).map_err(failure)?,
        )))
    }

    /// Matches the actual test invocation without introducing a doctest runtool.
    pub(crate) fn matches(&self, arguments: &[OsString], directory: &Path) -> bool {
        let option = |name: &str| {
            let mut values = arguments.iter().enumerate().filter_map(|(index, value)| {
                if value == name {
                    arguments.get(index + 1).and_then(|value| value.to_str())
                } else {
                    value.to_str()?.strip_prefix(&format!("{name}="))
                }
            });
            let first = values.next();
            (first, values.next().is_some())
        };
        let crate_name = option("--crate-name");
        let edition = option("--edition");
        arguments.iter().any(|value| value == "--test")
            && !arguments
                .iter()
                .any(|value| value.to_string_lossy().starts_with("--test-runtool"))
            && crate_name == (Some(self.crate_name.as_str()), false)
            && !edition.1
            && edition.0.unwrap_or("2015") == self.edition
            && arguments
                .iter()
                .filter(|value| !value.to_string_lossy().starts_with('-'))
                .filter(|value| {
                    std::fs::canonicalize(directory.join(value)).ok().as_deref()
                        == Some(self.source.as_path())
                })
                .count()
                == 1
    }
}

/// Projects only doctest-enabled libraries from the already-selected workspace packages.
pub(crate) fn project(packages: &[&Value], root: &Path) -> Result<Vec<Target>, PhaseFailure> {
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for package in packages {
        let name = package["name"]
            .as_str()
            .ok_or_else(|| failure("doctest package has no name"))?;
        if name == "terlan" {
            continue;
        }
        let targets = package["targets"]
            .as_array()
            .filter(|targets| targets.len() <= 4096)
            .ok_or_else(|| failure("missing or excessive Cargo target inventory"))?;
        for target in targets {
            let enabled = target["doctest"]
                .as_bool()
                .ok_or_else(|| failure("missing Cargo doctest policy"))?;
            if !enabled {
                continue;
            }
            let kinds = target["kind"]
                .as_array()
                .ok_or_else(|| failure("missing Cargo target kind"))?;
            if !kinds.iter().any(|kind| {
                matches!(
                    kind.as_str(),
                    Some("lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro")
                )
            }) {
                continue;
            }
            let path = |value: &Value| -> Result<PathBuf, PhaseFailure> {
                let path = value
                    .as_str()
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .ok_or_else(|| failure("Cargo doctest path is not absolute"))?;
                let resolved = std::fs::canonicalize(&path).map_err(failure)?;
                if !resolved.starts_with(root) || !resolved.is_file() {
                    return Err(failure("Cargo doctest input is outside the workspace"));
                }
                Ok(resolved)
            };
            let crate_name = target["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| failure("Cargo doctest target has no name"))?
                .replace('-', "_");
            let edition = target["edition"]
                .as_str()
                .filter(|edition| ["2015", "2018", "2021", "2024"].contains(edition))
                .ok_or_else(|| failure("Cargo doctest target has unsupported edition"))?;
            let entry = Target {
                package: name.into(),
                crate_name,
                manifest: path(&package["manifest_path"])?,
                source: path(&target["src_path"])?,
                edition: edition.into(),
            };
            if selected.len() == 256 || !seen.insert(entry.key()?) {
                return Err(failure("duplicate or excessive Cargo doctest targets"));
            }
            selected.push(entry);
        }
    }
    Ok(selected)
}

#[cfg(test)]
#[path = "targets_test.rs"]
mod tests;
