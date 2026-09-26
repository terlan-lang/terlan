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
