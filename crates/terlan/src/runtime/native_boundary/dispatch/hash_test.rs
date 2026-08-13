use super::*;

#[test]
fn rejects_malformed_and_parent_paths() {
    assert!(!valid_sha256("ABC"));
    assert!(!safe_relative_path("../escape"));
    assert!(!safe_relative_path("/absolute"));
    assert!(safe_relative_path("nested/value.txt"));
}
