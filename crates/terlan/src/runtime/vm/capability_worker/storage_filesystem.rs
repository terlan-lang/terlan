//! Conservative Linux admission for SQLite WAL's local durable directory.
//!
//! A filesystem type is not proof of stable hardware or correct mount options.
//! The administrator must still provide persistent media that honors fsync.

use std::io;
use std::path::Path;

use rustix::fs::{fstatfs, fstatvfs, open, FsWord, Mode, OFlags, StatVfsMountFlags};

// Linux UAPI include/uapi/linux/magic.h. The ext family shares one identity;
// this must not be described as an ext4-specific or journal-presence test.
const EXT_MAGIC: FsWord = 0xef53;
const XFS_MAGIC: FsWord = 0x5846_5342;
const BTRFS_MAGIC: FsWord = 0x9123_683e;

/// Rejects memory, network, overlay, unknown, and read-only storage before spawn.
pub(super) fn require_local_filesystem(directory: &Path) -> io::Result<()> {
    let directory = open(
        directory,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let filesystem = fstatfs(&directory)?;
    let mount = fstatvfs(&directory)?;
    admit_filesystem(
        filesystem.f_type,
        mount.f_flag.contains(StatVfsMountFlags::RDONLY),
    )
}

/// Uses an allowlist: an unrecognized filesystem cannot imply durability.
fn admit_filesystem(kind: FsWord, read_only: bool) -> io::Result<()> {
    if read_only {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "durable storage requires a writable filesystem",
        ));
    }
    if !matches!(kind, EXT_MAGIC | XFS_MAGIC | BTRFS_MAGIC) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "durable storage requires a local ext-family, XFS, or Btrfs filesystem; memory, network, overlay, and unknown filesystems are not admitted",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "storage_filesystem_test.rs"]
mod tests;
