//! Private, bounded, single-writer records for one Cargo native-test owner.

use crate::execution_environment::ExecutionEnvironment;
use crate::{file_identity, PhaseFailure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

const MAX_RECORD_BYTES: u64 = 1024 * 1024;

/// Registers exactly the disposable files permitted in a freshly reserved namespace.
pub(super) struct Registry {
    directory: Option<PathBuf>,
    leaves: BTreeSet<String>,
}

impl Registry {
    /// Reserves a private directory using the same allocator as direct test result logs.
    pub(super) fn create(environment: &ExecutionEnvironment) -> Result<Self, PhaseFailure> {
        let directory =
            crate::test_result_log::reserve_directory(environment, "rust-workspace-run")?;
        let mut registry = Self {
            directory: Some(directory),
            leaves: BTreeSet::new(),
        };
        registry.register("context.json");
        registry.register("cargo.json");
        Ok(registry)
    }

    /// The helper receives this exact namespace, never an environment-derived temporary root.
    pub(super) fn path(&self) -> &Path {
        self.directory.as_deref().expect("owned workspace registry")
    }

    /// Registers all potential files before publishing an executable authorization.
    pub(super) fn register_target(&mut self, key: &str) -> Result<(), PhaseFailure> {
        if self.leaves.len() >= 32 * 1024
            || key.len() != 64
            || !key.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(failure("invalid or over-budget workspace target record"));
        }
        for suffix in [
            "certificate.json",
            "started.json",
            "all.json",
            "ignored.json",
            "run.json",
            "complete.json",
        ] {
            self.register(&format!("{key}.{suffix}"));
        }
        self.leaves.insert(format!("{key}.libtest.log"));
        Ok(())
    }

    /// Registers a fixed observer executable before linking it into the owned namespace.
    pub(super) fn register_observer(&mut self, name: &str) -> Result<(), PhaseFailure> {
        if name.is_empty()
            || matches!(name, "." | "..")
            || name.len() > 128
            || self.leaves.len() >= 32 * 1024
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
        {
            return Err(failure("invalid observer filename"));
        }
        self.leaves.insert(name.into());
        Ok(())
    }

    fn register(&mut self, leaf: &str) {
        for suffix in ["", ".claim", ".pending"] {
            self.leaves.insert(format!("{leaf}{suffix}"));
        }
    }

    fn clean(&self) -> Result<(), PhaseFailure> {
        for leaf in &self.leaves {
            match fs::remove_file(self.path().join(leaf)) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(failure(error)),
            }
        }
        fs::remove_dir(self.path()).map_err(failure)
    }

    /// Retains available launch records on ordinary failure before deleting disposable files.
    /// Missing/interrupted records are not presented as a complete subprocess inventory.
    pub(super) fn failure_evidence(&self) -> Value {
        let control = ProcessControl::new(std::time::Duration::from_secs(5));
        let started = Instant::now();
        let mut launches = Vec::new();
        let mut errors = Vec::new();
        for leaf in &self.leaves {
            if ![".started.json", ".all.json", ".ignored.json", ".run.json"]
                .iter()
                .any(|suffix| leaf.ends_with(suffix))
            {
                continue;
            }
            if let Err(error) = control.check(started) {
                errors.push(error.detail);
                break;
            }
            let path = self.path().join(leaf);
            match path.try_exists() {
                Ok(false) => continue,
                Err(error) => {
                    errors.push(error.to_string());
                    continue;
                }
                Ok(true) => (),
            }
            match read_json(&path, control) {
                Ok((record, identity))
                    if record["pid"]
                        .as_u64()
                        .is_some_and(|pid| pid > 0 && pid <= u32::MAX.into()) =>
                {
                    launches.push(serde_json::json!({"record":leaf, "pid":record["pid"], "identity_sha256":identity}));
                }
                Ok(_) => errors.push(format!("invalid launch record {leaf}")),
                Err(error) => errors.push(error.detail),
            }
        }
        serde_json::json!({"scope":"retained-native-launch-records-v1", "decision":"fail", "complete":false,
            "nested_process_launch_count":launches.len(), "launches":launches, "observation_errors":errors})
    }

    /// Unknown siblings are not deleted; successful ownership requires complete cleanup.
    pub(super) fn close(mut self) -> Result<(), PhaseFailure> {
        self.clean()?;
        self.directory = None;
        Ok(())
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        if self.directory.is_some() {
            if let Err(error) = self.clean() {
                eprintln!(
                    "[rust-test-suite] workspace registry cleanup failed: {}",
                    error.detail
                );
            }
        }
    }
}

/// Selects one stable record key from Cargo's actual resolved executable path.
pub(super) fn executable_key(executable: &Path) -> Result<String, PhaseFailure> {
    let resolved = fs::canonicalize(executable).map_err(failure)?;
    Ok(file_identity::hex(Sha256::new_with_prefix(
        resolved.as_os_str().as_encoded_bytes(),
    )))
}

/// Uses a persistent exclusive claim and same-directory rename; this is not a reusable receipt.
pub(super) fn write_new(path: &Path, value: &Value) -> Result<(), PhaseFailure> {
    let bytes = serde_json::to_vec(value).map_err(failure)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(failure("workspace record exceeds its byte budget"));
    }
    let adjacent = |suffix: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    let create = |path: &Path| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path).map_err(failure)
    };
    drop(create(&adjacent(".claim"))?);
    if path.try_exists().map_err(failure)? {
        return Err(failure("workspace record already exists"));
    }
    let temporary = adjacent(".pending");
    let mut file = create(&temporary)?;
    file.write_all(&bytes).map_err(failure)?;
    drop(file);
    fs::rename(temporary, path).map_err(failure)
}

/// Reads exactly the guarded bytes whose identity the completion record carries.
pub(super) fn read_json(
    path: &Path,
    control: ProcessControl<'_>,
) -> Result<(Value, String), PhaseFailure> {
    let mut identity = Sha256::new();
    let bytes = file_identity::read_hashed_file(
        path,
        &mut identity,
        MAX_RECORD_BYTES,
        control,
        Instant::now(),
    )?;
    Ok((
        serde_json::from_slice(&bytes).map_err(failure)?,
        file_identity::hex(identity),
    ))
}

/// Attributes workspace protocol failures separately from a passing test exit.
pub(super) fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "workspace-test-owner-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "workspace_native_storage_test.rs"]
mod tests;
