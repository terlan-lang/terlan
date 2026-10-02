use std::path::Path;

#[test]
fn std_package_implementations_are_audited_without_build_outputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut files = Vec::new();
    super::collect_files(&root.join("std"), &mut files).unwrap();
    let paths: Vec<_> = files
        .iter()
        .map(|path| path.strip_prefix(root).unwrap().to_string_lossy())
        .collect();
    for required in [
        "std/lib.rs",
        "std/native/packages.rs",
        "std/db/native_operations.rs",
        "std/data/native/src/lib.rs",
        "std/net/native/src/lib.rs",
        "std/http/native/src/request.rs",
        "std/http/native/src/response_builder.rs",
        "std/http/native/src/source_descriptor/response.rs",
        "std/http/native/src/conversion_test.rs",
        "std/native/libpq/generated/native/rust/src/lib.rs",
    ] {
        assert!(
            paths.iter().any(|path| path == required),
            "missing {required}"
        );
    }
    assert!(!paths.iter().any(|path| path.starts_with("std/summaries/")));
    assert!(!paths
        .iter()
        .any(|path| path.contains("/target/") || path.contains("/_build/")));
}
