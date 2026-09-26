//! Bounded independent proof-replica receipts under one stable filesystem lease.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use super::{execution_signature, NormalizedExecution};
use crate::terlan_quality::QualityResult;

const MAX_RECEIPT: u64 = 16 * 1024 * 1024;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    input: String,
    replica: u8,
    signature: String,
    execution: NormalizedExecution,
}

/// Owns publication and eviction; never removes or replaces the stable lock.
pub(super) struct Checkpoints {
    path: PathBuf,
    _lease: File,
    protected: BTreeSet<String>,
    /// Verified receipts consumed, including recovered pending publication.
    pub(super) reused: usize,
    /// Newly completed and persisted replicas.
    pub(super) completed: usize,
}

impl Checkpoints {
    /// Serializes readers/writers before inspecting any cached receipt.
    pub(super) fn open(root: &Path, protected: BTreeSet<String>) -> QualityResult<Self> {
        Self::open_with_timeout(root, protected, Duration::from_secs(30))
    }

    fn open_with_timeout(
        root: &Path,
        protected: BTreeSet<String>,
        timeout: Duration,
    ) -> QualityResult<Self> {
        for input in &protected {
            receipt_name(input, 1)?;
        }
        let mut path = root.to_path_buf();
        for part in ["target", "quality", "proof-replay-cache", "v1"] {
            path.push(part);
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.to_string()),
            }
            if !fs::symlink_metadata(&path)
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                return Err("proof checkpoint parent must be a real directory".into());
            }
        }
        let lock = path.join("owner.lock");
        if lock
            .symlink_metadata()
            .is_ok_and(|metadata| !metadata.is_file())
        {
            return Err("proof checkpoint lock must be a regular file".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .map_err(|error| error.to_string())?;
        let start = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) if start.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => {
                    return Err(format!("cannot acquire proof checkpoint lease: {error}"))
                }
            }
        }
        let held = file.metadata().map_err(|error| error.to_string())?;
        let named = fs::symlink_metadata(&lock).map_err(|error| error.to_string())?;
        if !named.is_file() || held.dev() != named.dev() || held.ino() != named.ino() {
            return Err("proof checkpoint lock changed during acquisition".into());
        }
        let store = Self {
            path,
            _lease: file,
            protected,
            reused: 0,
            completed: 0,
        };
        store.prune(0)?;
        Ok(store)
    }

    /// Commits only a validated replica; failed producers leave no successful receipt.
    pub(super) fn run(
        &mut self,
        input: &str,
        replica: u8,
        execute: impl FnOnce() -> QualityResult<NormalizedExecution>,
    ) -> QualityResult<NormalizedExecution> {
        let name = receipt_name(input, replica)?;
        if !self.protected.contains(input) {
            return Err("proof inputs changed after checkpoint preflight".into());
        }
        let final_path = self.path.join(format!("{name}.json"));
        let pending = self.path.join(format!("{name}.pending"));
        if let Some(receipt) = read_receipt(&final_path, input, replica)? {
            File::open(&final_path)
                .and_then(|file| file.set_modified(SystemTime::now()))
                .map_err(|error| error.to_string())?;
            self.reused += 1;
            return Ok(receipt.execution);
        }
        // A complete pending write can survive parent SIGKILL before rename.
        if pending.try_exists().map_err(|error| error.to_string())? {
            match read_receipt(&pending, input, replica) {
                Ok(Some(receipt)) => {
                    fs::rename(&pending, &final_path).map_err(|error| error.to_string())?;
                    self.reused += 1;
                    return Ok(receipt.execution);
                }
                Err(_) | Ok(None) => {
                    require_regular(&pending)?;
                    fs::remove_file(&pending).map_err(|error| error.to_string())?;
                }
            }
        }
        let execution = execute()?;
        let receipt = Receipt {
            schema: "terlan.proof-replica.v1".into(),
            input: input.into(),
            replica,
            signature: execution_signature(&execution),
            execution,
        };
        let bytes = serde_json::to_vec(&receipt).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_RECEIPT {
            return Err("proof replica receipt exceeds output budget".into());
        }
        self.prune(bytes.len() as u64)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&pending, &final_path).map_err(|error| error.to_string())?;
        File::open(&self.path)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
        self.completed += 1;
        Ok(receipt.execution)
    }

    fn prune(&self, incoming: u64) -> QualityResult<()> {
        let mut rows = Vec::new();
        let mut bytes = incoming;
        for child in fs::read_dir(&self.path).map_err(|error| error.to_string())? {
            let child = child.map_err(|error| error.to_string())?;
            if child.file_name() == "owner.lock" {
                continue;
            }
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| "invalid proof cache name")?;
            if !owned_name(&name) {
                return Err("unexpected entry in proof checkpoint storage".into());
            }
            let metadata = require_regular(&child.path())?;
            if metadata.len() > MAX_RECEIPT || rows.len() >= 4096 {
                return Err("proof checkpoint storage exceeds inspection budget".into());
            }
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or("proof cache size overflow")?;
            rows.push((
                metadata.modified().map_err(|error| error.to_string())?,
                child.path(),
                metadata.len(),
                self.protected.contains(&name[..64]),
            ));
        }
        rows.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let mut count = rows.len() + usize::from(incoming != 0);
        for (modified, path, size, keep) in rows {
            let expired = SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default()
                > MAX_AGE;
            if !keep && (expired || bytes > MAX_BYTES || count > MAX_ENTRIES) {
                fs::remove_file(path).map_err(|error| error.to_string())?;
                bytes -= size;
                count -= 1;
            }
        }
        if bytes > MAX_BYTES || count > MAX_ENTRIES {
            return Err("protected proof receipts exceed retention budget".into());
        }
        Ok(())
    }
}

fn receipt_name(input: &str, replica: u8) -> QualityResult<String> {
    if input.len() != 64
        || !input
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || !matches!(replica, 1 | 2)
    {
        return Err("invalid proof replica identity".into());
    }
    Ok(format!("{input}-{replica}"))
}

fn owned_name(name: &str) -> bool {
    name.strip_suffix(".json")
        .or_else(|| name.strip_suffix(".pending"))
        .and_then(|stem| stem.rsplit_once('-'))
        .is_some_and(|(input, replica)| {
            matches!(replica, "1" | "2") && receipt_name(input, 1).is_ok()
        })
}

fn require_regular(path: &Path) -> QualityResult<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("proof checkpoint must be a regular file".into());
    }
    Ok(metadata)
}

fn read_receipt(path: &Path, input: &str, replica: u8) -> QualityResult<Option<Receipt>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    let metadata = require_regular(path)?;
    if metadata.len() > MAX_RECEIPT {
        return Err("proof receipt exceeds read budget".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_RECEIPT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err("proof receipt grew beyond read budget".into());
    }
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|error| format!("corrupt proof receipt {}: {error}", path.display()))?;
    if receipt.schema != "terlan.proof-replica.v1"
        || receipt.input != input
        || receipt.replica != replica
        || receipt.signature != execution_signature(&receipt.execution)
    {
        return Err("proof receipt identity or output hash mismatch".into());
    }
    Ok(Some(receipt))
}

#[cfg(test)]
#[path = "lean_proof_checkpoint_test.rs"]
mod tests;
