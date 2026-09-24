//! Filesystem admission checks do not equate process recovery with power-loss safety.

use super::*;

#[test]
fn local_filesystem_admission_is_closed_and_rejects_read_only_mounts() {
    for kind in [EXT_MAGIC, XFS_MAGIC, BTRFS_MAGIC] {
        assert!(admit_filesystem(kind, false).is_ok());
        assert_eq!(
            admit_filesystem(kind, true).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    // tmpfs, ramfs, NFS, SMB, CIFS, FUSE, overlayfs, 9p, and unknown.
    for kind in [
        0x0102_1994,
        0x8584_58f6,
        0x6969,
        0x517b,
        0xff53_4d42,
        0x6573_5546,
        0x794c_7630,
        0x0102_1997,
        0,
    ] {
        assert_eq!(
            admit_filesystem(kind, false).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }
}

#[test]
fn filesystem_probe_rejects_non_directories_and_symlinks() {
    let parent = tempfile::tempdir().unwrap();
    let file = parent.path().join("file");
    std::fs::write(&file, []).unwrap();
    assert!(require_local_filesystem(&file).is_err());
    let alias = parent.path().join("alias");
    std::os::unix::fs::symlink(parent.path(), &alias).unwrap();
    assert!(require_local_filesystem(&alias).is_err());
    assert!(require_local_filesystem(&parent.path().join("missing")).is_err());
}

#[test]
fn real_memory_filesystem_is_rejected_before_worker_binding() {
    use std::os::unix::fs::PermissionsExt;

    let memory = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in("/dev/shm")
        .expect("Linux shared-memory filesystem fixture");
    let directory = memory.path().canonicalize().unwrap();
    assert_eq!(
        require_local_filesystem(&directory).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    let scratch = super::super::VmCapabilityWorkerSandboxDir::create().unwrap();
    let error = super::super::linux_worker_command(
        &std::env::current_exe().unwrap(),
        &["storage".into()],
        scratch.path(),
        Some(&directory),
    )
    .expect_err("memory-backed durable binding must fail before spawn");
    assert!(error.contains("error[capability_worker.storage]"));
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
}
