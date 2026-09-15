//! Content observations for Cargo/Rustup configuration outside Git's inventory.

use crate::execution_environment::ExecutionEnvironment;
use crate::file_identity::{field, hex, read_hashed_file, same_file};
use crate::{process_failure, PhaseFailure};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

const MAX_PATHS: usize = 512;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[path = "cargo_configuration.rs"]
mod cargo_configuration;

#[derive(Clone, PartialEq, Eq)]
struct ConfigurationFile {
    path: PathBuf,
    present: bool,
    bytes: u64,
    identity: String,
    contents: Vec<u8>,
}

/// Retains both boundary observations; changed or missing closeout cannot pass.
#[derive(Default)]
pub(super) struct ToolConfiguration {
    before: Vec<ConfigurationFile>,
    after: Option<Vec<ConfigurationFile>>,
    cargo_order: Vec<PathBuf>,
    settings: crate::cargo_tool_settings::CargoToolSettings,
}

impl ToolConfiguration {
    /// Includes absent files and inactive legacy/modern alternatives conservatively.
    pub(super) fn capture(
        environment: &ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let started = Instant::now();
        let mut before = observe_started(&paths(environment)?, control, MAX_BYTES, started)?;
        let (cargo_order, settings) =
            cargo_configuration::expand(environment, &mut before, control, started)?;
        before.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self {
            before,
            after: None,
            cargo_order,
            settings,
        })
    }

    /// Returns settings projected while parsing the admitted configuration bytes.
    pub(super) fn cargo_settings(&self) -> &crate::cargo_tool_settings::CargoToolSettings {
        &self.settings
    }

    /// Checks the admitted configuration paths without rereading ambient settings.
    pub(super) fn verify(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        if !self.is_bound() || self.after.is_some() {
            return Err(failure(
                "configuration admission is missing or closeout repeated",
            ));
        }
        let paths = self.before.iter().map(|row| row.path.clone()).collect();
        self.after = Some(observe(&paths, control, MAX_BYTES)?);
        if !self.verified() {
            return Err(failure(
                "Cargo/Rustup configuration changed during execution",
            ));
        }
        Ok(())
    }

    /// Distinguishes generic test ledgers from configuration-bound execution.
    pub(super) fn is_bound(&self) -> bool {
        !self.before.is_empty()
    }

    /// Requires matching byte identities, including previously absent files.
    pub(super) fn verified(&self) -> bool {
        self.is_bound() && self.after.as_ref() == Some(&self.before)
    }

    /// Reports hashes, presence, and path identity without configuration contents.
    pub(super) fn json(&self) -> serde_json::Value {
        let rows =
            |values: &[ConfigurationFile]| {
                values.iter().map(|row| {
            let mut path_digest = Sha256::new();
            field(&mut path_digest, b"terlan.configuration-path.v1");
            field(&mut path_digest, row.path.as_os_str().as_encoded_bytes());
            serde_json::json!({
                "path": row.path.to_str(), "path_identity_sha256": hex(path_digest),
                "present": row.present, "bytes": row.bytes, "identity_sha256": row.identity,
            })
        }).collect::<Vec<_>>()
            };
        serde_json::json!({
            "scope": "cargo-rustup-configuration-files-v2",
            "before": rows(&self.before), "after": self.after.as_deref().map(rows),
            "cargo_load_order": self.cargo_order.iter().map(|path| path.to_str()).collect::<Vec<_>>(),
            "verified": self.verified(),
        })
    }
}

fn paths(environment: &ExecutionEnvironment) -> Result<BTreeSet<PathBuf>, PhaseFailure> {
    let root = home::env::Env::current_dir(environment).map_err(failure)?;
    let mut paths = BTreeSet::new();
    for (depth, ancestor) in root.ancestors().enumerate() {
        if depth >= 64 {
            return Err(failure("tool configuration exceeds its ancestor budget"));
        }
        paths.insert(ancestor.join(".cargo/config"));
        paths.insert(ancestor.join(".cargo/config.toml"));
        paths.insert(ancestor.join("rust-toolchain"));
        paths.insert(ancestor.join("rust-toolchain.toml"));
    }
    let cargo_home = root.join(home::env::cargo_home_with_env(environment).map_err(failure)?);
    paths.insert(cargo_home.join("config"));
    paths.insert(cargo_home.join("config.toml"));
    let rustup_home = root.join(home::env::rustup_home_with_env(environment).map_err(failure)?);
    paths.insert(rustup_home.join("settings.toml"));
    #[cfg(unix)]
    {
        paths.insert(PathBuf::from("/etc/rustup/settings.toml"));
        if let Some(path) = environment
            .value("RUSTUP_OVERRIDE_UNIX_FALLBACK_SETTINGS")
            .and_then(|value| value.into_string().ok())
        {
            paths.insert(root.join(path));
        }
    }
    Ok(paths)
}

fn observe(
    paths: &BTreeSet<PathBuf>,
    control: ProcessControl<'_>,
    budget: u64,
) -> Result<Vec<ConfigurationFile>, PhaseFailure> {
    observe_started(paths, control, budget, Instant::now())
}

fn observe_started(
    paths: &BTreeSet<PathBuf>,
    control: ProcessControl<'_>,
    budget: u64,
    started: Instant,
) -> Result<Vec<ConfigurationFile>, PhaseFailure> {
    if paths.is_empty() || paths.len() > MAX_PATHS {
        return Err(failure("invalid tool-configuration inventory size"));
    }
    let mut remaining = budget;
    let mut rows = Vec::with_capacity(paths.len());
    for path in paths {
        control.check(started).map_err(process_failure)?;
        let row = observe_file(path, control, started, remaining)?;
        remaining -= row.bytes;
        rows.push(row);
    }
    Ok(rows)
}

fn observe_file(
    path: &Path,
    control: ProcessControl<'_>,
    started: Instant,
    budget: u64,
) -> Result<ConfigurationFile, PhaseFailure> {
    let mut digest = Sha256::new();
    field(&mut digest, b"terlan.tool-configuration.v1");
    field(&mut digest, path.as_os_str().as_encoded_bytes());
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            field(&mut digest, b"absent");
            return Ok(ConfigurationFile {
                path: path.to_owned(),
                present: false,
                bytes: 0,
                identity: hex(digest),
                contents: Vec::new(),
            });
        }
        Err(error) => return Err(failure(error)),
    };
    if metadata.is_symlink() {
        field(&mut digest, b"symlink");
        field(
            &mut digest,
            fs::read_link(path)
                .map_err(failure)?
                .as_os_str()
                .as_encoded_bytes(),
        );
    } else {
        field(&mut digest, b"regular");
    }
    let resolved = fs::canonicalize(path).map_err(failure)?;
    field(&mut digest, resolved.as_os_str().as_encoded_bytes());
    let contents = read_hashed_file(&resolved, &mut digest, budget, control, started)?;
    if !same_file(&metadata, &fs::symlink_metadata(path).map_err(failure)?)
        || fs::canonicalize(path).map_err(failure)? != resolved
    {
        return Err(failure("tool configuration changed while being hashed"));
    }
    Ok(ConfigurationFile {
        path: path.to_owned(),
        present: true,
        bytes: contents.len() as u64,
        identity: hex(digest),
        contents,
    })
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "tool-configuration-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "tool_configuration_test.rs"]
mod tool_configuration_test;
