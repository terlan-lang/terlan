use super::package_relative_path;

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
