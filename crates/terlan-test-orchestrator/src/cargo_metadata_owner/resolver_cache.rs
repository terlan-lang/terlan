//! Shared bounded tree traversal with distinct resolver and package-source policies.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};
use sha2::Sha256;
use terlan_process_owner::ProcessControl;

use crate::execution_environment::ExecutionEnvironment;
use crate::file_identity::{field, hash_file, hex, same_file};
use crate::{process_failure, PhaseFailure};

const MAX_ENTRIES: usize = 262_144;
const MAX_NAMES: u64 = 32 * 1024 * 1024;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    sha256: String,
    entries: usize,
    name_bytes: u64,
    content_bytes: u64,
    covered: BTreeSet<PathBuf>,
}

impl Snapshot {
    /// Summarizes bytes actually hashed rather than referenced from another input owner.
    pub(super) fn json(&self) -> Value {
        json!({"sha256":self.sha256, "entries":self.entries, "name_bytes":self.name_bytes,
            "content_bytes":self.content_bytes, "hashed_files":self.covered.len()})
    }
}

/// Before/after cache observations share one frozen Cargo-home selection.
pub(super) struct Binding {
    home: PathBuf,
    before: Snapshot,
    after: Option<Snapshot>,
}

impl Binding {
    /// Shares admitted manifest/index paths so supplemental source reads do not repeat them.
    pub(super) fn covered_paths(&self) -> &BTreeSet<PathBuf> {
        &self.before.covered
    }

    pub(super) fn capture(
        environment: &ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let root = home::env::Env::current_dir(environment).map_err(failure)?;
        let home = root.join(home::env::cargo_home_with_env(environment).map_err(failure)?);
        Ok(Self {
            before: observe(&home, control, MAX_ENTRIES, MAX_BYTES)?,
            home,
            after: None,
        })
    }

    pub(super) fn verify(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        if self.after.is_some() {
            return Err(failure("Cargo resolver cache closeout cannot repeat"));
        }
        self.after = Some(observe(&self.home, control, MAX_ENTRIES, MAX_BYTES)?);
        if self.after.as_ref() != Some(&self.before) {
            return Err(failure(
                "Cargo resolver cache changed during metadata production",
            ));
        }
        Ok(())
    }

    pub(super) fn json(&self) -> Value {
        let row = |snapshot: &Snapshot| {
            json!({
                "sha256": snapshot.sha256, "entries": snapshot.entries,
                "name_bytes": snapshot.name_bytes, "content_bytes": snapshot.content_bytes,
            })
        };
        json!({
            "scope": "cargo-home-resolver-cache-v1",
            "before": row(&self.before), "after": self.after.as_ref().map(row),
            "verified": self.after.as_ref() == Some(&self.before),
        })
    }
}

#[derive(Clone, Copy)]
enum Contents<'a> {
    All,
    Manifests,
    Sources {
        covered: &'a BTreeSet<PathBuf>,
        excluded: &'a BTreeSet<PathBuf>,
    },
}

impl Contents<'_> {
    fn skips(self, path: &Path) -> bool {
        matches!(self, Self::Sources { excluded, .. } if path.file_name().is_some_and(|name| name == ".git") || excluded.iter().any(|root| path.starts_with(root)))
    }
}

struct Observer<'a> {
    digest: Sha256,
    control: ProcessControl<'a>,
    started: Instant,
    entries: usize,
    name_bytes: u64,
    content_bytes: u64,
    entry_limit: usize,
    byte_limit: u64,
    covered: BTreeSet<PathBuf>,
}

fn canonical_home(home: &Path) -> Result<Option<PathBuf>, PhaseFailure> {
    match fs::canonicalize(home) {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failure(error)),
    }
}

fn observe(
    home: &Path,
    control: ProcessControl<'_>,
    entry_limit: usize,
    byte_limit: u64,
) -> Result<Snapshot, PhaseFailure> {
    use sha2::Digest;
    let mut observer = Observer {
        digest: Sha256::new(),
        control,
        started: Instant::now(),
        entries: 0,
        name_bytes: 0,
        content_bytes: 0,
        entry_limit,
        byte_limit,
        covered: BTreeSet::new(),
    };
    control.check(observer.started).map_err(process_failure)?;
    field(&mut observer.digest, b"terlan.cargo-resolver-cache.v1");
    field(&mut observer.digest, home.as_os_str().as_encoded_bytes());
    let canonical = canonical_home(home)?;
    if let Some(root) = &canonical {
        field(&mut observer.digest, root.as_os_str().as_encoded_bytes());
        for (name, policy) in [
            ("registry/index", Contents::All),
            ("registry/src", Contents::Manifests),
            ("git/db", Contents::All),
            ("git/checkouts", Contents::Manifests),
        ] {
            let selected = canonical_home(&root.join(name))?;
            field(
                &mut observer.digest,
                selected
                    .as_deref()
                    .unwrap_or(Path::new("missing"))
                    .as_os_str()
                    .as_encoded_bytes(),
            );
            observer.reserve(Path::new(name))?;
            observer.visit(root, Path::new(name), policy, 0)?;
        }
    } else {
        field(&mut observer.digest, b"missing-cargo-home");
    }
    if canonical_home(home)? != canonical {
        return Err(failure("Cargo home changed during cache observation"));
    }
    Ok(Snapshot {
        sha256: hex(observer.digest),
        entries: observer.entries,
        name_bytes: observer.name_bytes,
        content_bytes: observer.content_bytes,
        covered: observer.covered,
    })
}

/// Supplemental package contents share the resolver's bounded traversal and guarded reader.
pub(super) fn observe_sources(
    roots: &[PathBuf],
    covered: &BTreeSet<PathBuf>,
    excluded: &BTreeSet<PathBuf>,
    control: ProcessControl<'_>,
) -> Result<Snapshot, PhaseFailure> {
    use sha2::Digest;
    let mut observer = Observer {
        digest: Sha256::new(),
        control,
        started: Instant::now(),
        entries: 0,
        name_bytes: 0,
        content_bytes: 0,
        entry_limit: MAX_ENTRIES,
        byte_limit: 8 * 1024 * 1024 * 1024,
        covered: BTreeSet::new(),
    };
    field(
        &mut observer.digest,
        b"terlan.cargo-package-supplemental-sources.v1",
    );
    for root in roots {
        control.check(observer.started).map_err(process_failure)?;
        field(&mut observer.digest, root.as_os_str().as_encoded_bytes());
        observer.reserve(root)?;
        observer.visit(
            root,
            Path::new(""),
            Contents::Sources { covered, excluded },
            0,
        )?;
    }
    Ok(Snapshot {
        sha256: hex(observer.digest),
        entries: observer.entries,
        name_bytes: observer.name_bytes,
        content_bytes: observer.content_bytes,
        covered: observer.covered,
    })
}

impl Observer<'_> {
    fn reserve(&mut self, relative: &Path) -> Result<(), PhaseFailure> {
        self.entries += 1;
        self.name_bytes += relative.as_os_str().as_encoded_bytes().len() as u64;
        if self.entries > self.entry_limit || self.name_bytes > MAX_NAMES {
            return Err(failure("Cargo resolver cache exceeds its traversal budget"));
        }
        Ok(())
    }

    fn visit(
        &mut self,
        root: &Path,
        relative: &Path,
        policy: Contents<'_>,
        depth: usize,
    ) -> Result<(), PhaseFailure> {
        self.control.check(self.started).map_err(process_failure)?;
        if depth > MAX_DEPTH {
            return Err(failure("Cargo resolver cache exceeds its traversal budget"));
        }
        field(&mut self.digest, relative.as_os_str().as_encoded_bytes());
        let path = root.join(relative);
        let before = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                field(&mut self.digest, b"missing");
                return Ok(());
            }
            Err(error) => return Err(failure(error)),
        };
        if before.is_dir() {
            field(&mut self.digest, b"directory");
            let mut names = Vec::new();
            for entry in fs::read_dir(&path).map_err(failure)? {
                self.control.check(self.started).map_err(process_failure)?;
                let child = relative.join(entry.map_err(failure)?.file_name());
                if policy.skips(&root.join(&child)) {
                    continue;
                }
                self.reserve(&child)?;
                names.push(child);
            }
            names.sort();
            for child in names {
                self.visit(root, &child, policy, depth + 1)?;
            }
            let after = fs::symlink_metadata(&path).map_err(failure)?;
            if !same_file(&before, &after) {
                return Err(failure(
                    "Cargo resolver directory changed while observing it",
                ));
            }
        } else if before.is_file() {
            field(&mut self.digest, b"file");
            if matches!(policy, Contents::Sources { covered, .. } if covered.contains(&path) || self.covered.contains(&path))
            {
                field(&mut self.digest, b"covered-by-input-owner");
            } else if matches!(policy, Contents::All | Contents::Sources { .. })
                || manifest(relative)
            {
                self.content_bytes += hash_file(
                    &path,
                    &mut self.digest,
                    self.byte_limit.saturating_sub(self.content_bytes),
                    self.control,
                    self.started,
                )?;
                self.covered.insert(path);
            }
        } else if before.is_symlink() && matches!(policy, Contents::Sources { .. }) {
            let resolved = fs::canonicalize(&path).map_err(failure)?;
            if !resolved.is_file() {
                return Err(failure(
                    "package source directory links require explicit ownership",
                ));
            }
            field(&mut self.digest, b"source-link");
            field(
                &mut self.digest,
                fs::read_link(&path)
                    .map_err(failure)?
                    .as_os_str()
                    .as_encoded_bytes(),
            );
            field(&mut self.digest, resolved.as_os_str().as_encoded_bytes());
            if matches!(policy, Contents::Sources { covered, .. } if covered.contains(&resolved) || self.covered.contains(&resolved))
            {
                field(&mut self.digest, b"covered-by-input-owner");
            } else {
                self.content_bytes += hash_file(
                    &resolved,
                    &mut self.digest,
                    self.byte_limit.saturating_sub(self.content_bytes),
                    self.control,
                    self.started,
                )?;
                self.covered.insert(resolved);
            }
            if !same_file(&before, &fs::symlink_metadata(&path).map_err(failure)?) {
                return Err(failure("package source link changed during observation"));
            }
        } else {
            return Err(failure(format!(
                "Cargo resolver cache contains an unsupported link or special file: {}",
                path.display()
            )));
        }
        Ok(())
    }
}

fn manifest(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            "Cargo.toml"
                | "Cargo.toml.orig"
                | "Cargo.lock"
                | "config"
                | "config.toml"
                | ".cargo-checksum.json"
        )
    )
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "resolver_cache_test.rs"]
mod tests;
