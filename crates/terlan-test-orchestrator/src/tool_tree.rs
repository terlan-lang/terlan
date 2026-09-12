//! Bounded content identity for an installed Rust toolchain's executable inputs.

use crate::file_identity::{field, hash_file, hex, same_file};
use crate::{process_failure, PhaseFailure};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

const MAX_ENTRIES: usize = 65_536;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// A content snapshot, not a version-string assertion or a transitive SDK seal.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ToolTree {
    root: PathBuf,
    identity: String,
    entries: usize,
    bytes: u64,
    scope: &'static str,
}

impl ToolTree {
    /// Hashes bin/lib, including rustlib targets, sources, and component manifests.
    pub(super) fn capture(root: &Path, control: ProcessControl<'_>) -> Result<Self, PhaseFailure> {
        capture(root, control, MAX_ENTRIES, MAX_BYTES)
    }

    /// Captures a custom compiler's default sysroot without requiring a compiler bin directory.
    pub(super) fn capture_sysroot(
        root: &Path,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        capture_directories(
            root,
            control,
            MAX_ENTRIES,
            MAX_BYTES,
            &["lib"],
            "terlan.rust-sysroot-lib.v1",
        )
    }

    /// Exposes the canonical root for reference-based identity reuse, not rehashing.
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    /// Renders the exact observed scope without claiming external loader closure.
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({"scope": self.scope, "root": self.root.to_str(), "identity_sha256": self.identity,
            "entries": self.entries, "bytes": self.bytes})
    }
}

struct Inventory<'a> {
    root: &'a Path,
    digest: Sha256,
    entries: usize,
    bytes: u64,
    entry_budget: usize,
    byte_budget: u64,
    control: ProcessControl<'a>,
    started: Instant,
}

fn capture(
    root: &Path,
    control: ProcessControl<'_>,
    entries: usize,
    bytes: u64,
) -> Result<ToolTree, PhaseFailure> {
    capture_directories(
        root,
        control,
        entries,
        bytes,
        &["bin", "lib"],
        "terlan.rust-toolchain-bin-lib.v1",
    )
}

fn capture_directories(
    root: &Path,
    control: ProcessControl<'_>,
    entries: usize,
    bytes: u64,
    directories: &[&str],
    scope: &'static str,
) -> Result<ToolTree, PhaseFailure> {
    let root = fs::canonicalize(root).map_err(failure)?;
    let mut inventory = Inventory {
        root: &root,
        digest: Sha256::new(),
        entries: 0,
        bytes: 0,
        entry_budget: entries,
        byte_budget: bytes,
        control,
        started: Instant::now(),
    };
    field(&mut inventory.digest, scope.as_bytes());
    field(&mut inventory.digest, root.as_os_str().as_encoded_bytes());
    for directory in directories {
        let path = root.join(directory);
        if !fs::symlink_metadata(&path).map_err(failure)?.is_dir() {
            return Err(failure("Rust toolchain requires real bin/lib directories"));
        }
        inventory.visit(&path, 0)?;
    }
    Ok(ToolTree {
        root: root.clone(),
        identity: hex(inventory.digest),
        entries: inventory.entries,
        bytes: inventory.bytes,
        scope,
    })
}

impl Inventory<'_> {
    fn visit(&mut self, path: &Path, depth: usize) -> Result<(), PhaseFailure> {
        self.control.check(self.started).map_err(process_failure)?;
        if depth > 64 || self.entries >= self.entry_budget {
            return Err(failure("Rust toolchain exceeds its traversal budget"));
        }
        self.entries += 1;
        field(
            &mut self.digest,
            path.strip_prefix(self.root)
                .map_err(failure)?
                .as_os_str()
                .as_encoded_bytes(),
        );
        let before = fs::symlink_metadata(path).map_err(failure)?;
        if before.is_dir() {
            field(&mut self.digest, b"directory");
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                field(&mut self.digest, &before.mode().to_le_bytes());
            }
            #[cfg(not(unix))]
            field(
                &mut self.digest,
                &[u8::from(before.permissions().readonly())],
            );
            let mut entries = Vec::new();
            for entry in fs::read_dir(path).map_err(failure)? {
                if entries.len() >= self.entry_budget - self.entries {
                    return Err(failure("Rust toolchain directory exceeds its entry budget"));
                }
                entries.push(entry.map_err(failure)?.path());
            }
            entries.sort();
            for child in entries {
                self.visit(&child, depth + 1)?;
            }
        } else {
            let resolved = fs::canonicalize(path).map_err(failure)?;
            if !resolved.starts_with(self.root) {
                return Err(failure("Rust toolchain link escapes its installation"));
            }
            if before.is_symlink() {
                field(&mut self.digest, b"symlink");
                field(
                    &mut self.digest,
                    fs::read_link(path)
                        .map_err(failure)?
                        .as_os_str()
                        .as_encoded_bytes(),
                );
            } else {
                field(&mut self.digest, b"file");
            }
            field(
                &mut self.digest,
                resolved
                    .strip_prefix(self.root)
                    .map_err(failure)?
                    .as_os_str()
                    .as_encoded_bytes(),
            );
            self.bytes += hash_file(
                &resolved,
                &mut self.digest,
                self.byte_budget - self.bytes,
                self.control,
                self.started,
            )?;
            if fs::canonicalize(path).map_err(failure)? != resolved {
                return Err(failure("Rust toolchain link changed during hashing"));
            }
        }
        if !same_file(&before, &fs::symlink_metadata(path).map_err(failure)?) {
            return Err(failure("Rust toolchain changed while being hashed"));
        }
        Ok(())
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "toolchain-identity-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "tool_tree_test.rs"]
mod tool_tree_test;
