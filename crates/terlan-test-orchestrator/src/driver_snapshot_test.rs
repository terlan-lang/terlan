use super::*;
use crate::test_orchestrator_test::temporary_fixture;

fn executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn snapshot_is_independent_and_unchanged_install_preserves_the_file() {
    let root = temporary_fixture("driver-independent");
    let source = root.0.join("build-output");
    let destination = root.0.join("driver");
    executable(&source, b"first executable");
    assert!(install(&source, &destination).unwrap());
    let before = fs::metadata(&destination).unwrap();
    assert!(!install(&source, &destination).unwrap());
    assert!(same_file(&before, &fs::metadata(&destination).unwrap()));
    executable(&source, b"rebuilt executable");
    assert_eq!(fs::read(&destination).unwrap(), b"first executable");
    assert!(install(&source, &destination).unwrap());
    assert_eq!(fs::read(&destination).unwrap(), b"rebuilt executable");
}

#[test]
fn concurrent_readers_prevent_snapshot_replacement_until_both_finish() {
    let root = temporary_fixture("driver-readers");
    let source = root.0.join("build-output");
    let destination = root.0.join("driver");
    executable(&source, b"first executable");
    install(&source, &destination).unwrap();
    let first = read_lease(&destination).unwrap().unwrap();
    let second = read_lease(&destination).unwrap().unwrap();
    executable(&source, b"rebuilt executable");
    assert!(install(&source, &destination).is_err());
    drop(first);
    assert!(install(&source, &destination).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"first executable");
    drop(second);
    assert!(install(&source, &destination).unwrap());
}

#[test]
fn writer_excludes_readers_and_ordinary_build_outputs_need_no_lease() {
    let root = temporary_fixture("driver-writer");
    let destination = root.0.join("driver");
    assert!(read_lease(&destination).unwrap().is_none());
    let writer = ReportFile::open(&destination).unwrap();
    assert!(read_lease(&destination).is_err());
    drop(writer);
    assert!(read_lease(&destination).unwrap().is_some());
}

#[test]
fn invalid_driver_sources_cannot_replace_a_valid_snapshot() {
    let root = temporary_fixture("driver-invalid");
    let source = root.0.join("build-output");
    let destination = root.0.join("driver");
    executable(&source, b"executable");
    install(&source, &destination).unwrap();
    assert!(install(&source, &source).is_err());
    executable(&source, b"");
    assert!(install(&source, &destination).is_err());
    fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_len(MAX_DRIVER_BYTES + 1)
        .unwrap();
    assert!(install(&source, &destination).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"executable");
}

#[cfg(unix)]
#[test]
fn snapshot_preserves_and_repairs_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = temporary_fixture("driver-permissions");
    let source = root.0.join("build-output");
    let destination = root.0.join("driver");
    executable(&source, b"executable");
    install(&source, &destination).unwrap();
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(install(&source, &destination).unwrap());
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o755
    );
    fs::set_permissions(&source, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(install(&source, &destination).is_err());
}

#[cfg(unix)]
#[test]
fn snapshot_rejects_source_destination_and_lock_symlinks() {
    let root = temporary_fixture("driver-symlinks");
    let source = root.0.join("build-output");
    let alias = root.0.join("alias");
    let destination = root.0.join("driver");
    executable(&source, b"executable");
    std::os::unix::fs::symlink(&source, &alias).unwrap();
    assert!(install(&alias, &destination).is_err());
    assert!(install(&source, &alias).is_err());
    std::os::unix::fs::symlink(&source, root.0.join("driver.lock")).unwrap();
    assert!(read_lease(&destination).is_err());
    assert!(install(&source, &destination).is_err());
    assert_eq!(fs::read(&source).unwrap(), b"executable");
}

#[test]
fn interrupted_install_retains_old_bytes_until_replacement_is_complete() {
    let root = temporary_fixture("driver-interrupted");
    let source = root.0.join("build-output");
    let destination = root.0.join("driver");
    executable(&source, b"first executable");
    install(&source, &destination).unwrap();
    fs::write(root.0.join("driver.pending"), b"partial executable").unwrap();
    assert!(!install(&source, &destination).unwrap());
    assert_eq!(fs::read(&destination).unwrap(), b"first executable");
    assert_eq!(
        fs::read(root.0.join("driver.interrupted")).unwrap(),
        b"partial executable"
    );
}
