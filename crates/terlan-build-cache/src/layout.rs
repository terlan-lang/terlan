//! Narrow Rust 1.96 incremental layout; never recursively traverse cache contents.
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const RETIRED: &str = ".terlan-incremental-retired-v1";
pub(crate) const OWNER_LOCK: &str = ".terlan-incremental-retention.lock";
const MAX_CHILDREN: usize = 32_768;
const RUST_HEADER: &[u8] = b"RSIC\0\0\x1d1.96.0 (ac68faa20 2026-05-25)";

/// A parsed session name also identifies rustc's shared/exclusive lock file.
#[derive(Clone, Debug)]
pub(crate) struct SessionName {
    pub(crate) lock: String,
    pub(crate) created: SystemTime,
    pub(crate) working: bool,
}

/// Parse the versioned cache name without accepting traversal or extra components.
pub(crate) fn session_name(name: &str) -> io::Result<SessionName> {
    let parts: Vec<_> = name.split('-').collect();
    let ["s", timestamp, random, hash] = parts.as_slice() else {
        return Err(invalid(name));
    };
    if !base36(timestamp) || !base36(random) || !base36(hash) {
        return Err(invalid(name));
    }
    let micros = u64::from_str_radix(timestamp, 36).map_err(|_| invalid(name))?;
    let created = UNIX_EPOCH
        .checked_add(Duration::from_micros(micros))
        .ok_or_else(|| invalid(name))?;
    Ok(SessionName {
        lock: format!("s-{timestamp}-{random}.lock"),
        created,
        working: *hash == "working",
    })
}

/// Recognize a rustc crate-name/stable-ID directory component.
pub(crate) fn crate_name(name: &str) -> bool {
    name.rsplit_once('-').is_some_and(|(name, hash)| {
        !name.is_empty()
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && base36(hash)
    })
}

/// Recognize a session lock, including locks rustc has retained after its own GC.
pub(crate) fn lock_name(name: &str) -> bool {
    name.strip_suffix(".lock")
        .is_some_and(|base| session_name(&format!("{base}-working")).is_ok())
}

fn base36(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Attribute a rejected layout without treating it as disposable data.
pub(crate) fn invalid(value: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!(
        "unrecognized or unsafe incremental cache layout: {value}"
    ))
}

/// Validate every ancestor below the repository; symlinked targets are not adopted.
pub(crate) fn profile_root(repo: &Path) -> io::Result<PathBuf> {
    if !fs::symlink_metadata(repo.join("Cargo.toml"))?.is_file()
        || !repo.join(".git").try_exists()?
    {
        return Err(invalid("expected a Git/Cargo repository root"));
    }
    let target = repo.join("target");
    directory(&target)?;
    let profile = target.join("debug");
    directory(&profile)?;
    directory(&profile.join("incremental"))?;
    Ok(profile)
}

/// Check existence without following even a dangling symbolic link.
pub(crate) fn present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Require a real directory, not a symlink to one.
pub(crate) fn directory(path: &Path) -> io::Result<()> {
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(invalid(path.display()));
    }
    Ok(())
}

/// Require a regular file without dereferencing symbolic links.
pub(crate) fn regular(path: &Path) -> io::Result<Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(invalid(path.display()));
    }
    Ok(metadata)
}

/// Compare Linux file identities across open/lock/inspection boundaries.
pub(crate) fn same_file(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// Acquire an existing rustc lock, never create a replacement for a missing lock.
pub(crate) fn lease(path: &Path) -> io::Result<Option<Lease>> {
    let before = regular(path)?;
    let file = File::open(path)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
    }
    if !same_file(&before, &file.metadata()?) || !same_file(&before, &regular(path)?) {
        return Err(invalid(format!("lock replaced: {}", path.display())));
    }
    Ok(Some(Lease(file)))
}

/// Explicitly release before close: a concurrent spawn can temporarily inherit
/// the same open file description until exec, despite close-on-exec descriptors.
pub(crate) struct Lease(File);

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
#[path = "lease_test.rs"]
mod lease_test;

/// Bound each flat namespace scan and return deterministic names.
pub(crate) fn children(path: &Path) -> io::Result<Vec<PathBuf>> {
    directory(path)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        entries.push(entry?.path());
        if entries.len() > MAX_CHILDREN {
            return Err(invalid(format!("scan bound exceeded: {}", path.display())));
        }
    }
    entries.sort();
    Ok(entries)
}

/// Reject non-UTF8 names, which rustc's known cache layout does not generate.
pub(crate) fn name(path: &Path) -> io::Result<&str> {
    path.file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| invalid(path.display()))
}

/// Return validated flat files. Partial retirement may already lack its header.
pub(crate) fn payload(path: &Path, require_header: bool) -> io::Result<Vec<(PathBuf, Metadata)>> {
    let mut files = Vec::new();
    for file in children(path)? {
        let name = name(&file)?;
        let known = matches!(
            name,
            "dep-graph.bin"
                | "dep-graph.part.bin"
                | "metadata.rmeta"
                | "query-cache.bin"
                | "work-products.bin"
        ) || name.strip_suffix(".o").is_some_and(base36);
        if !known {
            return Err(invalid(file.display()));
        }
        let metadata = regular(&file)?;
        files.push((file, metadata));
    }
    if require_header {
        let mut header = vec![0; RUST_HEADER.len()];
        File::open(path.join("dep-graph.bin"))?.read_exact(&mut header)?;
        if header != RUST_HEADER {
            return Err(invalid(format!(
                "unsupported Rust cache version: {}",
                path.display()
            )));
        }
    }
    Ok(files)
}

/// Unlink only validated flat retired files; never recursively remove directories.
pub(crate) fn remove_payload(path: &Path) -> io::Result<()> {
    // Validate the entire directory before the first unlink, also on recovery.
    let files = payload(path, false)?;
    for (file, metadata) in files {
        if !same_file(&metadata, &regular(&file)?) {
            return Err(invalid(format!(
                "retired file replaced: {}",
                file.display()
            )));
        }
        fs::remove_file(file)?;
    }
    fs::remove_dir(path)
}

#[cfg(test)]
pub(crate) fn write_header(path: &Path) {
    fs::write(path.join("dep-graph.bin"), RUST_HEADER).unwrap();
}
