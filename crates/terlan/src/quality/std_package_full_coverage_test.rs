use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies executable tests and generated contracts pass the coverage gate.
///
/// Inputs:
/// - One ordinary std API row backed by `@test`.
/// - One generated JS surface row backed by an unannotated contract function.
///
/// Output:
/// - Summary counting one executable row and one generated contract row.
///
/// Transformation:
/// - Locks the two legal release API manifest row categories.
#[test]
fn std_package_coverage_accepts_tests_and_generated_contracts() {
    let root = temp_repo("std_package_coverage_accepts");
    write(
        &root,
        RELEASE_API_TESTS,
        "std.core.Bool.is_true\tstd/core/BoolTest.terl\tis_true_accepts_true\n\
         std.js.Array.generated_surface\tstd/js/ArrayTest.terl\tgenerated_surface_contract\n",
    );
    write_release_manifest(&root, &["std.core.Bool", "std.js.Array"]);
    write(
        &root,
        "std/core/BoolTest.terl",
        "module std.core.BoolTest.\n\n@test\npub is_true_accepts_true(): Bool ->\n    Bool.is_true(true).\n",
    );
    write(
        &root,
        "std/js/ArrayTest.terl",
        "module std.js.ArrayTest.\n\npub generated_surface_contract(): Bool ->\n    true.\n",
    );

    let summary = run_std_package_coverage_100(&root).expect("coverage should pass");

    assert_eq!(summary.api_row_count, 2);
    assert_eq!(summary.executable_test_row_count, 1);
    assert_eq!(summary.generated_contract_row_count, 1);
    assert_eq!(summary.release_module_count, 2);
    assert_eq!(summary.uncovered_module_baseline_count, 0);
}

/// Verifies non-generated rows cannot point at unannotated functions.
///
/// Inputs:
/// - One ordinary std API row backed by a public function without `@test`.
///
/// Output:
/// - Diagnostic naming the missing annotated test.
///
/// Transformation:
/// - Prevents ordinary std API coverage from degrading into compile-surface
///   function declarations.
#[test]
fn std_package_coverage_rejects_unannotated_non_generated_rows() {
    let root = temp_repo("std_package_coverage_rejects_unannotated");
    write(
        &root,
        RELEASE_API_TESTS,
        "std.core.Bool.is_true\tstd/core/BoolTest.terl\tis_true_accepts_true\n",
    );
    write_release_manifest(&root, &["std.core.Bool"]);
    write(
        &root,
        "std/core/BoolTest.terl",
        "module std.core.BoolTest.\n\npub is_true_accepts_true(): Bool ->\n    true.\n",
    );

    let error = run_std_package_coverage_100(&root).expect_err("coverage should fail");

    assert!(error.contains("missing @test function `is_true_accepts_true`"));
}

/// Verifies generated rows cannot re-enter executable test accounting.
///
/// Inputs:
/// - One generated JS row backed by an `@test` function.
///
/// Output:
/// - Diagnostic explaining generated rows must use contracts.
///
/// Transformation:
/// - Keeps generated interface conformance outside std executable test counts.
#[test]
fn std_package_coverage_rejects_generated_rows_marked_as_tests() {
    let root = temp_repo("std_package_coverage_rejects_generated_test");
    write(
        &root,
        RELEASE_API_TESTS,
        "std.js.Array.generated_surface\tstd/js/ArrayTest.terl\tgenerated_surface_contract\n",
    );
    write_release_manifest(&root, &["std.js.Array"]);
    write(
        &root,
        "std/js/ArrayTest.terl",
        "module std.js.ArrayTest.\n\n@test\npub generated_surface_contract(): Bool ->\n    true.\n",
    );

    let error = run_std_package_coverage_100(&root).expect_err("coverage should fail");

    assert!(error.contains("must use an unannotated contract"));
}

/// Verifies duplicate API ids are rejected.
///
/// Inputs:
/// - Two manifest rows with the same API id.
///
/// Output:
/// - Diagnostic naming the duplicate API id.
///
/// Transformation:
/// - Prevents later manifest rows from silently masking earlier API coverage.
#[test]
fn std_package_coverage_rejects_duplicate_api_ids() {
    let root = temp_repo("std_package_coverage_rejects_duplicates");
    write(
        &root,
        RELEASE_API_TESTS,
        "std.core.Bool.is_true\tstd/core/BoolTest.terl\tis_true_accepts_true\n\
         std.core.Bool.is_true\tstd/core/BoolTest.terl\tis_true_accepts_true\n",
    );
    write_release_manifest(&root, &["std.core.Bool"]);
    write(
        &root,
        "std/core/BoolTest.terl",
        "module std.core.BoolTest.\n\n@test\npub is_true_accepts_true(): Bool ->\n    Bool.is_true(true).\n",
    );

    let error = run_std_package_coverage_100(&root).expect_err("coverage should fail");

    assert!(error.contains("duplicate API id `std.core.Bool.is_true`"));
}

/// Verifies duplicate release manifest modules are rejected.
///
/// Inputs:
/// - One release manifest with the same module listed twice.
///
/// Output:
/// - Diagnostic naming the duplicate release module.
///
/// Transformation:
/// - Prevents duplicate release rows from distorting module coverage counts.
#[test]
fn std_package_coverage_rejects_duplicate_release_modules() {
    let root = temp_repo("std_package_coverage_rejects_duplicate_modules");
    write(
        &root,
        RELEASE_API_TESTS,
        "std.core.Bool.is_true\tstd/core/BoolTest.terl\tis_true_accepts_true\n",
    );
    write_release_manifest(&root, &["std.core.Bool", "std.core.Bool"]);
    write(
        &root,
        "std/core/BoolTest.terl",
        "module std.core.BoolTest.\n\n@test\npub is_true_accepts_true(): Bool ->\n    Bool.is_true(true).\n",
    );

    let error = run_std_package_coverage_100(&root).expect_err("coverage should fail");

    assert!(error.contains("duplicate release module `std.core.Bool`"));
}

/// Verifies release modules without rows fail when they are not baselined.
///
/// Inputs:
/// - One release manifest module with no API coverage row.
///
/// Output:
/// - Diagnostic naming the uncovered module.
///
/// Transformation:
/// - Prevents new std modules from being added to the release manifest without
///   any executable coverage row.
#[test]
fn std_package_coverage_rejects_unbaselined_module_without_rows() {
    let root = temp_repo("std_package_coverage_rejects_uncovered_module");
    write(&root, RELEASE_API_TESTS, "");
    write_release_manifest(&root, &["std.core.Bool"]);

    let error = run_std_package_coverage_100(&root).expect_err("coverage should fail");

    assert!(error.contains("release module `std.core.Bool` has no release API coverage row"));
}

/// Verifies release API rows must point at adjacent std test files.
///
/// Inputs:
/// - A release API manifest row pointing outside `std/**Test.terl`.
///
/// Output:
/// - Diagnostic explaining the path contract before any file read occurs.
///
/// Transformation:
/// - Locks the release gate to std-adjacent executable tests and prevents
///   coverage rows from being satisfied by generic repo tests.
#[test]
fn std_package_coverage_rejects_non_std_test_paths() {
    let root = temp_repo("std_package_coverage_rejects_non_std_path");
    write(
        &root,
        RELEASE_API_TESTS,
        "std.core.Bool.is_true\ttests/std/BoolTest.terl\tis_true_accepts_true\n",
    );
    write_release_manifest(&root, &["std.core.Bool"]);

    let error = run_std_package_coverage_100(&root).expect_err("coverage should fail");

    assert!(error.contains("test path must be adjacent std `*Test.terl`"));
}

/// Verifies negative diagnostic tests cannot satisfy positive API coverage.
///
/// Inputs:
/// - A release API manifest row pointing at `std/**/negative/**Test.terl`.
///
/// Output:
/// - Diagnostic explaining the adjacent std positive-test contract.
///
/// Transformation:
/// - Keeps adversarial negative API coverage separate from positive release
///   API coverage accounting.
#[test]
fn std_package_coverage_rejects_negative_std_test_paths() {
    let root = temp_repo("std_package_coverage_rejects_negative_path");
    write(
        &root,
        RELEASE_API_TESTS,
        "std.core.Bool.is_true\tstd/core/negative/BoolTest.terl\tis_true_accepts_true\n",
    );
    write_release_manifest(&root, &["std.core.Bool"]);

    let error = run_std_package_coverage_100(&root).expect_err("coverage should fail");

    assert!(error.contains("test path must be adjacent std `*Test.terl`"));
}

/// Verifies the coverage gate accepts an empty release manifest with no gaps.
///
/// Inputs:
/// - Empty release API and release module manifests.
///
/// Output:
/// - Successful summary with no API rows, release modules, or baseline gaps.
///
/// Transformation:
/// - Locks the post-baseline state where release modules without coverage rows
///   fail instead of being tolerated by a shrink-only baseline.
#[test]
fn std_package_coverage_accepts_empty_baseline_state() {
    let root = temp_repo("std_package_coverage_accepts_empty_baseline");
    write(&root, RELEASE_API_TESTS, "");
    write_release_manifest(&root, &[]);

    let summary = run_std_package_coverage_100(&root).expect("empty baseline should pass");

    assert_eq!(summary.api_row_count, 0);
    assert_eq!(summary.release_module_count, 0);
    assert_eq!(summary.uncovered_module_baseline_count, 0);
}

/// Writes one fixture file, creating parents first.
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("fixture path has parent")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}

/// Writes a compact std release manifest for fixture modules.
fn write_release_manifest(root: &Path, modules: &[&str]) {
    let mut text = String::from("# fixture release manifest\n");
    for module in modules {
        let source = module.replace('.', "/");
        text.push_str(&format!(
            "module\t{module}\t{source}.terl\tstd/summaries/{module}.typi\ttests/std/RELEASE_API_TESTS.tsv\t{module}.html\n"
        ));
    }
    write(root, RELEASE_MANIFEST, &text);
}

/// Creates an isolated temporary repository fixture directory.
fn temp_repo(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(&path).expect("create temp repo");
    path
}
