//! Content identity and mutation checks for a pinned Linux proof toolchain.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::terlan_quality::QualityResult;

const MAX_ENTRIES: usize = 32768;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Stable content reference passed by a freshly executed admission boundary.
/// Restoring it hashes the real files again, never trusting serialized timestamps.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reference {
    tree: PathBuf,
    extras: Vec<PathBuf>,
    digest: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    kind: u8,
    bytes: u64,
    modified: Option<SystemTime>,
    identity: (u64, u64, u32, i64, i64),
}

/// A full byte digest is captured once; ctime/inode checks guard each execution.
pub(super) struct ToolFiles {
    tree: PathBuf,
    extras: Vec<PathBuf>,
    entries: BTreeMap<PathBuf, Stamp>,
    digest: String,
    racy: Vec<(PathBuf, [u8; 32])>,
}

impl ToolFiles {
    /// The admitted executable must be a captured native tool, not an extra path.
    pub(super) fn contains_tool(&self, path: &Path) -> bool {
        path.parent() == Some(self.tree.join("bin").as_path())
            && self
                .entries
                .get(path)
                .is_some_and(|entry| entry.kind == b'f' && entry.identity.2 & 0o111 != 0)
    }

    /// Describe content without capture-time timestamps or mutable cache state.
    pub(super) fn reference(&self) -> Reference {
        Reference {
            tree: self.tree.clone(),
            extras: self.extras.clone(),
            digest: self.digest.clone(),
        }
    }

    /// Revalidate admitted content without rerunning executable tool probes.
    pub(super) fn resume(reference: Reference) -> QualityResult<Self> {
        if !reference.tree.is_absolute()
            || reference.extras.len() > MAX_ENTRIES
            || reference.extras.iter().any(|path| !path.is_absolute())
        {
            return Err("invalid admitted proof tool paths".into());
        }
        let files = Self::capture(&reference.tree, &reference.extras)?;
        if files.digest != reference.digest {
            return Err("proof tool content changed after admission".into());
        }
        Ok(files)
    }

    /// Hashes the entire installed project plus explicitly resolved host inputs.
    pub(super) fn capture(tree: &Path, extras: &[PathBuf]) -> QualityResult<Self> {
        // A just-written file can change again within one filesystem clock tick.
        // Retain byte checks for that entire context, even after the tick passes.
        let recent = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs()
            .saturating_sub(4);
        let recent = i64::try_from(recent).map_err(|error| error.to_string())?;
        let tree = fs::canonicalize(tree).map_err(|error| error.to_string())?;
        let entries = inventory(&tree, extras)?;
        let mut digest = Sha256::new();
        digest.update(b"terlan.lean-tool-files.v1\0");
        let mut total = 0_u64;
        let mut racy = Vec::new();
        for (path, before) in &entries {
            let path_bytes = path.as_os_str().as_encoded_bytes();
            digest.update((path_bytes.len() as u64).to_le_bytes());
            digest.update(path_bytes);
            digest.update([before.kind]);
            digest.update(before.identity.2.to_le_bytes());
            match before.kind {
                b'f' => {
                    let bytes = file_digest(path, &mut total)?;
                    digest.update(bytes);
                    if before.identity.3 >= recent {
                        racy.push((path.clone(), bytes));
                    }
                }
                b'l' => digest.update(
                    fs::read_link(path)
                        .map_err(|error| error.to_string())?
                        .as_os_str()
                        .as_encoded_bytes(),
                ),
                _ => {}
            }
            if &stamp(path)? != before {
                return Err(format!(
                    "proof tool input changed during hashing: {}",
                    path.display()
                ));
            }
        }
        let result = Self {
            tree,
            extras: extras.to_vec(),
            entries,
            digest: super::super::format_digest(&digest.finalize()),
            racy,
        };
        result.verify_unchanged()?;
        Ok(result)
    }

    /// Returns the byte-based identity, not a timestamp-based cache key.
    pub(super) fn digest(&self) -> &str {
        &self.digest
    }

    /// Rejects writes, replacement, permissions, additions and removed inputs.
    pub(super) fn verify_unchanged(&self) -> QualityResult<()> {
        if inventory(&self.tree, &self.extras)? != self.entries {
            return Err(
                "proof tool inputs changed during execution; retry with fresh tool identity".into(),
            );
        }
        let mut bytes = 0;
        for (path, expected) in &self.racy {
            if &file_digest(path, &mut bytes)? != expected {
                return Err("proof tool input changed within its filesystem timestamp tick".into());
            }
        }
        Ok(())
    }
}

fn stamp(path: &Path) -> QualityResult<Stamp> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Stamp {
                kind: b'-',
                bytes: 0,
                modified: None,
                identity: (0, 0, 0, 0, 0),
            })
        }
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let kind = if metadata.is_file() {
        b'f'
    } else if metadata.is_dir() {
        b'd'
    } else if metadata.is_symlink() {
        b'l'
    } else {
        return Err(format!("unsupported proof tool input: {}", path.display()));
    };
    Ok(Stamp {
        kind,
        bytes: metadata.len(),
        modified: metadata.modified().ok(),
        identity: (
            metadata.dev(),
            metadata.ino(),
            metadata.mode(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ),
    })
}

fn inventory(tree: &Path, extras: &[PathBuf]) -> QualityResult<BTreeMap<PathBuf, Stamp>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![(tree.to_path_buf(), 0)];
    let mut bytes = 0_u64;
    while let Some((path, depth)) = pending.pop() {
        if depth > 64 {
            return Err("proof tool tree depth budget exceeded".into());
        }
        let state = stamp(&path)?;
        if state.kind == b'-' {
            return Err("proof tool tree disappeared".into());
        }
        if state.kind == b'd' {
            for child in fs::read_dir(&path).map_err(|error| error.to_string())? {
                pending.push((child.map_err(|error| error.to_string())?.path(), depth + 1));
                if pending.len() + result.len() > MAX_ENTRIES {
                    return Err("proof tool entry budget exceeded".into());
                }
            }
        } else if state.kind == b'l' {
            let target = fs::canonicalize(&path).map_err(|error| error.to_string())?;
            if !target.starts_with(tree) || !target.is_file() {
                return Err(
                    "proof tool symlink must resolve to a file within its pinned tree".into(),
                );
            }
        }
        insert(&mut result, path, state, &mut bytes)?;
    }
    for extra in extras {
        if !extra.is_absolute() {
            return Err("proof tool dependency must be absolute".into());
        }
        let state = stamp(extra)?;
        if state.kind == b'd' {
            return Err("extra proof tool dependency must be a file".into());
        }
        if state.kind != b'-' {
            let target = fs::canonicalize(extra).map_err(|error| error.to_string())?;
            insert(&mut result, target.clone(), stamp(&target)?, &mut bytes)?;
        }
        insert(&mut result, extra.clone(), state, &mut bytes)?;
        // Preserve linker and executable aliases such as /lib -> /usr/lib.
        for parent in extra.ancestors().skip(1) {
            let state = stamp(parent)?;
            if state.kind == b'l' {
                insert(&mut result, parent.to_path_buf(), state, &mut bytes)?;
            }
        }
    }
    Ok(result)
}

fn insert(
    entries: &mut BTreeMap<PathBuf, Stamp>,
    path: PathBuf,
    state: Stamp,
    bytes: &mut u64,
) -> QualityResult<()> {
    if entries.contains_key(&path) {
        return Ok(());
    }
    if state.kind == b'f' {
        *bytes = bytes
            .checked_add(state.bytes)
            .ok_or("proof tool size overflow")?;
    }
    if entries.len() >= MAX_ENTRIES || *bytes > MAX_BYTES {
        return Err("proof tool input budget exceeded".into());
    }
    entries.insert(path, state);
    Ok(())
}

fn file_digest(path: &Path, total: &mut u64) -> QualityResult<[u8; 32]> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        *total = total
            .checked_add(count as u64)
            .ok_or("proof tool size overflow")?;
        if *total > MAX_BYTES {
            return Err("proof tool byte budget exceeded during hashing".into());
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

#[cfg(test)]
#[path = "lean_proof_tool_files_test.rs"]
mod tests;
