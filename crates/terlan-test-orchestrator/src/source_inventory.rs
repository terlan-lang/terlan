//! Content binding for Git-listed working-tree inputs, not complete tool identity.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::process::Command;
use std::time::Instant;

use sha2::{Digest, Sha256};
use terlan_process_owner::ProcessControl;

use crate::file_identity::{field, hash_file, hex, same_file};
use crate::{process_failure, PhaseFailure};

const MAX_PATHS: usize = 50_000;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Exact source bytes and modes observed before or after suite execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceSnapshot {
    /// Domain-separated digest of names, presence, links, modes, and contents.
    pub(super) sha256: String,
    /// Git-listed tracked and nonignored untracked input names.
    pub(super) files: usize,
    /// Streamed regular-file contents, excluding generated ignored storage.
    pub(super) bytes: u64,
    /// Canonical files whose contents are already owned by this source observation.
    pub(super) covered_paths: BTreeSet<PathBuf>,
}

/// Both boundary observations; absence is never treated as verified source input.
#[derive(Default)]
pub(super) struct SourceBinding {
    /// Admission snapshot, recorded before the first build or test producer.
    pub(super) before: Option<SourceSnapshot>,
    /// Closeout snapshot, including changed inputs retained for diagnosis.
    pub(super) after: Option<SourceSnapshot>,
}

impl SourceBinding {
    /// Requires two real, equal observations rather than a caller's Boolean flag.
    pub(super) fn verified(&self) -> bool {
        self.before.is_some() && self.before == self.after
    }

    /// Renders source evidence without implying tool identity or reusable test success.
    pub(super) fn json(&self) -> serde_json::Value {
        let row = |snapshot: &SourceSnapshot| {
            serde_json::json!({
                "sha256": snapshot.sha256, "files": snapshot.files, "bytes": snapshot.bytes,
            })
        };
        serde_json::json!({
            "scope": "git-listed-working-tree-files-v1",
            "before": self.before.as_ref().map(row),
            "after": self.after.as_ref().map(row),
            "verified": self.verified(),
        })
    }
}

/// Captures actual working files, not index blobs or an unchecked commit string.
#[cfg(test)]
pub(super) fn capture(
    root: &Path,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
) -> Result<SourceSnapshot, PhaseFailure> {
    capture_with_git(
        root,
        Path::new("git"),
        &crate::execution_environment::ExecutionEnvironment::capture()?,
        control,
        launched,
    )
}

/// Uses the driver's admitted Git entry point without a second PATH resolution.
pub(super) fn capture_with_git(
    root: &Path,
    git: &Path,
    environment: &crate::execution_environment::ExecutionEnvironment,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
) -> Result<SourceSnapshot, PhaseFailure> {
    let started = Instant::now();
    let root = fs::canonicalize(root).map_err(failure)?;
    let mut command = environment.command(git);
    command.arg("-C").arg(&root).args([
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
    ]);
    let output = control
        .capture_stdout(&mut command, 16 * 1024 * 1024, launched)
        .map_err(process_failure)?;
    fingerprint(&root, &output, control, started, MAX_BYTES)
}

fn paths(output: &[u8]) -> Result<BTreeSet<&str>, PhaseFailure> {
    if output.is_empty() || !output.ends_with(&[0]) {
        return Err(failure("source inventory is empty or not NUL-terminated"));
    }
    let mut paths = BTreeSet::new();
    for bytes in output[..output.len() - 1].split(|byte| *byte == 0) {
        let path = std::str::from_utf8(bytes).map_err(failure)?;
        if path.is_empty()
            || path.contains('\\')
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
            || Path::new(path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(failure(
                "source inventory contains a noncanonical relative path",
            ));
        }
        paths.insert(path);
        if paths.len() > MAX_PATHS {
            return Err(failure("source inventory exceeds 50,000 names"));
        }
    }
    Ok(paths)
}

fn fingerprint(
    root: &Path,
    output: &[u8],
    control: ProcessControl<'_>,
    started: Instant,
    budget: u64,
) -> Result<SourceSnapshot, PhaseFailure> {
    let paths = paths(output)?;
    let mut digest = Sha256::new();
    field(&mut digest, b"terlan.rust-suite-working-source.v1");
    let mut bytes = 0;
    let mut covered_paths = BTreeSet::new();
    for path in &paths {
        control.check(started).map_err(process_failure)?;
        field(&mut digest, path.as_bytes());
        let path = root.join(path);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                field(&mut digest, b"absent");
                continue;
            }
            Err(error) => return Err(failure(error)),
        };
        if metadata.is_symlink() {
            field(&mut digest, b"symlink");
            field(
                &mut digest,
                fs::read_link(&path)
                    .map_err(failure)?
                    .as_os_str()
                    .as_encoded_bytes(),
            );
        } else {
            field(&mut digest, b"regular");
        }
        let resolved = fs::canonicalize(&path).map_err(failure)?;
        if !resolved.starts_with(root) {
            return Err(failure("source link escapes the working tree"));
        }
        let read = hash_file(
            &resolved,
            &mut digest,
            budget.saturating_sub(bytes),
            control,
            started,
        )?;
        if !same_file(&metadata, &fs::symlink_metadata(&path).map_err(failure)?) {
            return Err(failure("source changed while being hashed"));
        }
        bytes += read;
        covered_paths.insert(resolved);
    }
    Ok(SourceSnapshot {
        sha256: hex(digest),
        files: paths.len(),
        bytes,
        covered_paths,
    })
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "source-inventory-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "source_inventory_test.rs"]
mod tests;
