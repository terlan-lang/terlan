use super::*;

#[test]
fn repositories_with_identical_labels_are_isolated_and_cleaned_up() {
    let first = TestRepo::fixture("same-label");
    let second = TestRepo::fixture("same-label");
    assert_ne!(first.root(), second.root());
    first.write("nested/file.txt", "first").expect("write");
    second.write_fixture("nested/file.txt", "second");
    let first_path = first.root().to_path_buf();
    drop(first);
    assert!(!first_path.exists());
    assert_eq!(
        fs::read_to_string(second.root().join("nested/file.txt")).expect("read"),
        "second"
    );
    let second_path = second.root().to_path_buf();
    drop(second);
    assert!(!second_path.exists());
}

#[test]
fn failed_fixture_write_propagates_io_error() {
    let repo = TestRepo::fixture("failed-write");
    repo.write_fixture("parent", "not a directory");
    assert!(repo.write("parent/child", "data").is_err());
}

#[test]
fn fixture_is_cleaned_up_during_unwinding() {
    let repo = TestRepo::fixture("unwind");
    let path = repo.root().to_path_buf();
    assert!(std::panic::catch_unwind(|| {
        let _repo = repo;
        panic!("fixture test panic");
    })
    .is_err());
    assert!(!path.exists());
}

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
