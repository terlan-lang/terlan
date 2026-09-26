//! Shared bounded, cancellation-aware reads for source and executable identities.

use crate::{process_failure, PhaseFailure};
use sha2::{Digest, Sha256};
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::Path;
use std::time::Instant;
use terlan_process_owner::ProcessControl;

/// Hashes file size, permissions, and bytes under one execution deadline.
pub(super) fn hash_file(
    path: &Path,
    digest: &mut Sha256,
    budget: u64,
    control: ProcessControl<'_>,
    started: Instant,
) -> Result<u64, PhaseFailure> {
    visit_file(path, digest, budget, control, started, true, |_| {})
}

/// Hashes only content bytes through the same guarded reader, without retaining a copy.
pub(super) fn hash_file_contents(
    path: &Path,
    digest: &mut Sha256,
    budget: u64,
    control: ProcessControl<'_>,
    started: Instant,
) -> Result<u64, PhaseFailure> {
    visit_file(path, digest, budget, control, started, false, |_| {})
}

/// Retains exactly the bytes hashed in one bounded read, avoiding a second open.
pub(super) fn read_hashed_file(
    path: &Path,
    digest: &mut Sha256,
    budget: u64,
    control: ProcessControl<'_>,
    started: Instant,
) -> Result<Vec<u8>, PhaseFailure> {
    let mut bytes = Vec::new();
    visit_file(path, digest, budget, control, started, true, |chunk| {
        bytes.extend_from_slice(chunk);
    })?;
    Ok(bytes)
}

fn visit_file(
    path: &Path,
    digest: &mut Sha256,
    budget: u64,
    control: ProcessControl<'_>,
    started: Instant,
    include_metadata: bool,
    mut consume: impl FnMut(&[u8]),
) -> Result<u64, PhaseFailure> {
    control.check(started).map_err(process_failure)?;
    let mut file = regular_file(path)?;
    let before = file.metadata().map_err(failure)?;
    if before.len() > budget {
        return Err(failure("input exceeds its byte budget"));
    }
    if include_metadata {
        field(digest, &before.len().to_le_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            field(digest, &before.mode().to_le_bytes());
        }
        #[cfg(not(unix))]
        field(digest, &[u8::from(before.permissions().readonly())]);
    }
    let mut read = 0;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        control.check(started).map_err(process_failure)?;
        let count = file.read(&mut buffer).map_err(failure)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > before.len() {
            return Err(failure("input grew while being hashed"));
        }
        digest.update(&buffer[..count]);
        consume(&buffer[..count]);
    }
    if read != before.len()
        || !same_file(&before, &file.metadata().map_err(failure)?)
        || !same_file(&before, &fs::metadata(path).map_err(failure)?)
    {
        return Err(failure("input changed while being hashed"));
    }
    Ok(read)
}

fn regular_file(path: &Path) -> Result<File, PhaseFailure> {
    if !fs::symlink_metadata(path).map_err(failure)?.is_file() {
        return Err(failure("input is not a regular file"));
    }
    // NONBLOCK prevents a concurrently substituted FIFO from hanging open.
    #[cfg(unix)]
    let file = File::from(
        rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )
        .map_err(failure)?,
    );
    #[cfg(not(unix))]
    let file = File::open(path).map_err(failure)?;
    if !file.metadata().map_err(failure)?.is_file() {
        return Err(failure("opened input is not regular"));
    }
    Ok(file)
}

/// Detects replacement and metadata changes around a bounded byte read.
pub(super) fn same_file(left: &Metadata, right: &Metadata) -> bool {
    let same = left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
        && left.file_type() == right.file_type();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        same && (
            left.dev(),
            left.ino(),
            left.mode(),
            left.ctime(),
            left.ctime_nsec(),
        ) == (
            right.dev(),
            right.ino(),
            right.mode(),
            right.ctime(),
            right.ctime_nsec(),
        )
    }
    #[cfg(not(unix))]
    {
        same && left.permissions().readonly() == right.permissions().readonly()
    }
}

/// Prefixes variable-length identity fields to prevent ambiguous concatenation.
pub(super) fn field(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

/// Encodes the pinned SHA-2 implementation's digest without an extra dependency.
pub(super) fn hex(digest: Sha256) -> String {
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "input-identity-failed",
        detail: detail.to_string(),
    }
}
