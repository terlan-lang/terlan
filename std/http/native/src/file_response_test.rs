use super::{package_relative_path, read_response_file};
use std::path::Path;

#[test]
fn file_reads_preserve_exact_bytes_and_explicit_metadata() {
    let root = tempfile::tempdir().unwrap();
    let bytes = [0, 255, 10, 42];
    std::fs::write(root.path().join("body.txt"), bytes).unwrap();
    for (supplied, expected) in [
        ("", "text/plain; charset=utf-8"),
        ("application/custom", "application/custom"),
    ] {
        let (content_type, body) =
            read_response_file(Some(root.path()), &[], "body.txt", supplied.into()).unwrap();
        assert_eq!(content_type, expected);
        assert_eq!(body, bytes);
    }
}

#[cfg(unix)]
#[test]
fn file_read_errors_remain_errors_after_path_admission() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("unreadable.txt");
    std::fs::write(&file, "private").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let host_read = std::fs::read(&file);
    let response = read_response_file(Some(root.path()), &[], "unreadable.txt", String::new());
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    match host_read {
        Err(error) => {
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
            let error = response.unwrap_err();
            assert_eq!(error.code(), "http.file");
            assert!(error.message().contains("cannot be read"));
        }
        // Privileged hosts can read mode-000 files; the adapter adds no bypass.
        Ok(bytes) => assert_eq!(response.unwrap().1, bytes),
    }
}

#[test]
fn file_access_requires_context_and_rejects_non_files() {
    let root = tempfile::tempdir().unwrap();
    let error = read_response_file(None, &[], "body.txt", String::new()).unwrap_err();
    assert_eq!(error.code(), "http.file");
    assert_eq!(error.status(), 500);
    assert!(error.message().contains("file-serving context"));
    for path in ["", ".", "missing.txt"] {
        let error = read_response_file(Some(root.path()), &[], path, String::new()).unwrap_err();
        assert!(
            error.message().contains("does not name a file"),
            "{error:?}"
        );
    }
    for path in ["../outside", "a/../../outside", "a\\b", "a\0b"] {
        let error = read_response_file(Some(root.path()), &[], path, String::new()).unwrap_err();
        assert!(
            error.message().contains("not package-relative"),
            "{error:?}"
        );
    }
}

#[test]
fn trusted_roots_are_explicit_scoped_and_do_not_expand_relative_access() {
    let parent = tempfile::tempdir().unwrap();
    let package = parent.path().join("package");
    let trusted = parent.path().join("trusted");
    let neighbor = parent.path().join("trusted-neighbor");
    for root in [&package, &trusted, &neighbor] {
        std::fs::create_dir(root).unwrap();
        std::fs::write(root.join("file.txt"), "body").unwrap();
    }
    let read = |path: &Path, roots: &[std::path::PathBuf]| {
        read_response_file(Some(&package), roots, path.to_str().unwrap(), String::new())
    };
    let file = trusted.join("file.txt");
    assert!(read(&file, &[]).is_err());
    assert!(read(&file, std::slice::from_ref(&neighbor)).is_err());
    assert_eq!(
        read(&file, &[parent.path().join("missing"), trusted.clone()])
            .unwrap()
            .1,
        b"body"
    );
    assert!(read(&neighbor.join("file.txt"), std::slice::from_ref(&trusted)).is_err());
    assert!(read(&trusted.join("missing.txt"), std::slice::from_ref(&trusted)).is_err());
    assert!(read(
        Path::new("../trusted/file.txt"),
        std::slice::from_ref(&trusted)
    )
    .is_err());
    // A grant on one call does not change any subsequent caller's capabilities.
    assert!(read(&file, &[]).is_err());
}

#[cfg(unix)]
#[test]
fn trusted_roots_reject_escaping_symlinks_and_accept_canonical_roots() {
    use std::os::unix::fs::symlink;
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("trusted");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("inside.txt"), "inside").unwrap();
    std::fs::write(parent.path().join("outside.txt"), "outside").unwrap();
    symlink(parent.path().join("outside.txt"), root.join("escape.txt")).unwrap();
    symlink("inside.txt", root.join("internal.txt")).unwrap();
    symlink(&root, parent.path().join("linked-root")).unwrap();
    let roots = [parent.path().join("linked-root")];
    for (file, accepted) in [("escape.txt", false), ("internal.txt", true)] {
        let path = root.join(file);
        assert_eq!(
            read_response_file(
                Some(parent.path()),
                &roots,
                path.to_str().unwrap(),
                String::new()
            )
            .is_ok(),
            accepted
        );
    }
    symlink("loop", root.join("loop")).unwrap();
    assert!(package_relative_path(&root, "loop").is_none());
}

#[test]
fn package_paths_preserve_existing_and_missing_file_behavior() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let file = root.path().join("assets/body.txt");
    std::fs::write(&file, "body").unwrap();
    assert_eq!(
        package_relative_path(root.path(), "./assets/body.txt"),
        Some(file.canonicalize().unwrap())
    );
    assert_eq!(
        package_relative_path(root.path(), "missing/body.txt"),
        Some(root.path().join("missing/body.txt"))
    );
    assert!(package_relative_path(&root.path().join("missing"), "body.txt").is_none());
    for path in [
        "../outside",
        "/outside",
        "assets/../../outside",
        "assets\\body.txt",
        "assets/\0body.txt",
    ] {
        assert!(
            package_relative_path(root.path(), path).is_none(),
            "{path:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn package_paths_reject_symlink_escapes_but_allow_internal_links() {
    use std::os::unix::fs::symlink;
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("package");
    let outside = parent.path().join("package-neighbor");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(root.join("body.txt"), "inside").unwrap();
    std::fs::write(outside.join("body.txt"), "outside").unwrap();
    symlink(outside.join("body.txt"), root.join("file-escape")).unwrap();
    symlink(&outside, root.join("directory-escape")).unwrap();
    symlink("body.txt", root.join("internal")).unwrap();
    for path in ["file-escape", "directory-escape/body.txt"] {
        assert!(package_relative_path(&root, path).is_none());
    }
    assert_eq!(
        package_relative_path(&root, "internal"),
        Some(root.join("body.txt").canonicalize().unwrap())
    );
    symlink(&root, parent.path().join("linked-package")).unwrap();
    assert_eq!(
        package_relative_path(&parent.path().join("linked-package"), "body.txt"),
        Some(root.join("body.txt").canonicalize().unwrap())
    );
}
