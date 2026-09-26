//! Pins the validation driver independently of Cargo's mutable executable outputs.

use crate::file_identity::{read_hashed_file, same_file};
use crate::report_file::{read_lease, ReportFile, ReportLease};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

const MAX_DRIVER_BYTES: u64 = 32 * 1024 * 1024;

/// Holds the running snapshot's read lease through dispatch and every owned child.
pub(super) fn running_lease() -> Result<Option<ReportLease>, String> {
    let before = std::env::current_exe().map_err(|error| error.to_string())?;
    let lease = read_lease(&before)?;
    // A rename between exec and locking must not admit the new path's bytes as
    // the identity of old code. Linux reports a deleted executable's old dentry.
    let after = std::env::current_exe().map_err(|error| error.to_string())?;
    if before != after || !fs::symlink_metadata(&after).is_ok_and(|value| value.is_file()) {
        return Err("running validation driver was replaced before admission".into());
    }
    Ok(lease)
}

/// Installs an already compiled driver; this mode never starts Cargo or the suite.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        let destination = arguments
            .next()
            .ok_or("missing driver snapshot destination")?;
        if arguments.next().is_some() {
            return Err("unexpected driver snapshot arguments".into());
        }
        let source = std::env::current_exe().map_err(|error| error.to_string())?;
        install(&source, &executable_path(PathBuf::from(destination)))
    })();
    match result {
        Ok(changed) => {
            println!(
                "[rust-driver] snapshot {}",
                if changed { "installed" } else { "unchanged" }
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[rust-driver] {error}");
            ExitCode::FAILURE
        }
    }
}

fn executable_path(mut path: PathBuf) -> PathBuf {
    if !std::env::consts::EXE_SUFFIX.is_empty() && path.extension().is_none() {
        path.set_extension(std::env::consts::EXE_EXTENSION);
    }
    path
}

fn install(source: &Path, destination: &Path) -> Result<bool, String> {
    if fs::canonicalize(source).ok() == fs::canonicalize(destination).ok() {
        return Err("driver snapshot must be separate from its build output".into());
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("driver source must be a nonempty regular executable".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("driver source is not executable".into());
        }
    }
    let mut output = ReportFile::open_bounded(destination, MAX_DRIVER_BYTES)?;
    let started = Instant::now();
    let control = ProcessControl::new(Duration::from_secs(30));
    let mut source_hash = Sha256::new();
    let bytes = read_hashed_file(source, &mut source_hash, MAX_DRIVER_BYTES, control, started)
        .map_err(|error| error.detail)?;
    if !same_file(
        &metadata,
        &fs::symlink_metadata(source).map_err(|error| error.to_string())?,
    ) {
        return Err("driver source changed before installation".into());
    }
    if destination.exists() {
        let mut current_hash = Sha256::new();
        crate::file_identity::hash_file(
            destination,
            &mut current_hash,
            MAX_DRIVER_BYTES,
            control,
            started,
        )
        .map_err(|error| error.detail)?;
        if source_hash.finalize() == current_hash.finalize() {
            return Ok(false);
        }
    }
    output.publish_executable(&bytes, metadata.permissions())?;
    Ok(true)
}

#[cfg(test)]
#[path = "driver_snapshot_test.rs"]
mod tests;
