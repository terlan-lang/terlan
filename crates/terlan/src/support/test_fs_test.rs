use super::*;

#[test]
fn scoped_test_directory_cleans_normal_exit_and_explicit_close() {
    let directory = TestDirectory::new("scope", "normal");
    let path = directory.path().to_path_buf();
    fs::write(directory.join("partial.o"), b"partial object").unwrap();
    drop(directory);
    assert!(!path.exists());
    let directory = TestDirectory::new("scope", "close");
    let path = directory.path().to_path_buf();
    directory.close();
    assert!(!path.exists());
}

#[test]
fn scoped_test_directory_cleans_partial_builds_during_unwind() {
    let mut path = PathBuf::new();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let directory = TestDirectory::new("scope", "panic");
        path = directory.path().to_path_buf();
        fs::create_dir(directory.join("build")).unwrap();
        fs::write(directory.join("build/partial.o"), b"partial object").unwrap();
        panic!("injected test failure");
    }));
    assert!(failure.is_err());
    assert!(!path.exists());
}

#[test]
fn scoped_test_directory_rejects_path_labels_before_creation() {
    for name in ["", "../outside", "/", "nested/path", "nested\\path"] {
        assert!(std::panic::catch_unwind(|| TestDirectory::new("scope", name)).is_err());
    }
}
