use super::*;
use crate::support::test_fs::TestDirectory;
use std::os::unix::fs::{symlink, PermissionsExt};

fn fixture() -> TestDirectory {
    let root = TestDirectory::new("proof_tools", "files");
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(root.join("lib/kernel"), b"pinned kernel").unwrap();
    root
}

#[test]
fn tool_files_digest_binds_bytes_and_permissions_not_timestamps() {
    let root = fixture();
    let before = ToolFiles::capture(&root, &[]).unwrap();
    let path = root.join("lib/kernel");
    fs::File::open(&path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH)
        .unwrap();
    assert!(before.verify_unchanged().is_err());
    assert_eq!(
        before.digest(),
        ToolFiles::capture(&root, &[]).unwrap().digest()
    );
    fs::write(&path, b"changed bytes").unwrap();
    let changed = ToolFiles::capture(&root, &[]).unwrap();
    assert_ne!(before.digest(), changed.digest());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(changed.verify_unchanged().is_err());
    assert_ne!(
        changed.digest(),
        ToolFiles::capture(&root, &[]).unwrap().digest()
    );
}

#[test]
fn tool_files_detect_import_addition_and_previously_absent_host_configuration() {
    let root = fixture();
    let host = TestDirectory::new("proof_tools", "host");
    let absent = host.join("loader-config");
    let files = ToolFiles::capture(&root, std::slice::from_ref(&absent)).unwrap();
    fs::write(&absent, b"new loader policy").unwrap();
    assert!(files.verify_unchanged().is_err());
    let files = ToolFiles::capture(&root, &[absent]).unwrap();
    fs::write(root.join("lib/additional"), b"new import").unwrap();
    assert!(files.verify_unchanged().is_err());
}

#[test]
fn tool_files_bind_internal_aliases_and_reject_escaping_links() {
    let root = fixture();
    symlink("kernel", root.join("lib/alias")).unwrap();
    let files = ToolFiles::capture(&root, &[]).unwrap();
    files.verify_unchanged().unwrap();
    let outside = TestDirectory::new("proof_tools", "outside");
    fs::write(outside.join("library"), b"external").unwrap();
    symlink(outside.join("library"), root.join("lib/escape")).unwrap();
    assert!(ToolFiles::capture(&root, &[]).is_err());
}

#[test]
fn tool_files_bind_resolved_external_libraries() {
    let root = fixture();
    let outside = TestDirectory::new("proof_tools", "external");
    fs::write(outside.join("library"), b"external").unwrap();
    symlink("library", outside.join("alias")).unwrap();
    let files = ToolFiles::capture(&root, &[outside.join("alias")]).unwrap();
    fs::write(outside.join("library"), b"modified").unwrap();
    assert!(
        files.verify_unchanged().is_err(),
        "before={:?}; after={:?}",
        files.entries,
        inventory(&files.tree, &files.extras).unwrap()
    );
}

#[test]
fn tool_files_guard_same_size_writes_with_racy_timestamps() {
    let root = fixture();
    for _ in 0..32 {
        let path = root.join("lib/kernel");
        fs::write(&path, b"first").unwrap();
        let files = ToolFiles::capture(&root, &[]).unwrap();
        assert!(!files.racy.is_empty());
        fs::write(&path, b"other").unwrap();
        assert!(files.verify_unchanged().is_err());
    }
}

#[test]
fn tool_files_reject_oversized_sparse_input_before_hashing() {
    let root = fixture();
    fs::File::create(root.join("huge"))
        .unwrap()
        .set_len(MAX_BYTES + 1)
        .unwrap();
    assert!(ToolFiles::capture(&root, &[]).is_err());
}
