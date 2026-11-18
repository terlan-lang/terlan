use super::is_test_only_rust_source;
use std::path::Path;

#[test]
fn production_revision_excludes_every_supported_test_only_shape() {
    for path in [
        "src/compiler/native_ir/source_constructor_test.rs",
        "src/compiler/native_ir/native_object_test_support.rs",
        "src/compiler/native_ir/minimal_test_fixture.rs",
        "src/compiler/native_ir/try_region_test/case.rs",
        "src/compiler/native_ir/try_region_tests/case.rs",
        "tests/compiler.rs",
    ] {
        assert!(is_test_only_rust_source(Path::new(path)), "{path}");
    }
}

#[test]
fn production_revision_retains_non_test_sources_with_similar_words() {
    for path in [
        "src/compiler/native_ir/source_constructor.rs",
        "src/quality/otp_test_pipeline_inventory.rs",
        "src/quality/package_release_test_matrix.rs",
        "src/commands/test/runner.rs",
        "src/contest.rs",
    ] {
        assert!(!is_test_only_rust_source(Path::new(path)), "{path}");
    }
}
