use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies the std test honesty gate rejects fake test shapes.
///
/// Inputs:
/// - A temporary std tree with literal boolean, assert-true, identity
///   assertion, generated-surface, and declaration-surface tests.
///
/// Output:
/// - Diagnostics naming each fake test function.
///
/// Transformation:
/// - Proves fake-test detection is driven by parsed syntax output instead of
///   ad hoc source regexes.
#[test]
fn std_test_honesty_rejects_fake_test_patterns() {
    let root = make_quality_temp_dir("std_test_honesty_fake_patterns");
    write_file(
        &root,
        "std/sample/FakeTest.terl",
        r#"
module sample.FakeTest.

@test
pub literal_true(): Bool ->
    true.

@test
pub trivial_conjunction(): Bool ->
    true && true.

@test
pub trivial_and_conjunction(): Bool ->
    true and true.

@test
pub direct_assert_true(): Bool ->
    std.test.Test.assert(true).

@test
pub direct_assert_false_false(): Bool ->
    std.test.Test.assert_false(false).

@test
pub identity_assertion(): Bool ->
    std.test.Test.assert_equal(1, 1).

@test
pub generated_surface_is_declared(): Bool ->
    SomeApi.exists().

@test
pub router_surface_is_declared(): Bool ->
    SomeApi.exists().
"#,
    );

    let diagnostic = run_std_test_honesty_for_dir(&root, Path::new("std")).expect_err("fake tests");

    for name in [
        "literal_true",
        "trivial_conjunction",
        "trivial_and_conjunction",
        "direct_assert_true",
        "direct_assert_false_false",
        "identity_assertion",
        "generated_surface_is_declared",
        "router_surface_is_declared",
    ] {
        assert!(
            diagnostic.contains(name),
            "expected diagnostic for {name}: {diagnostic}"
        );
    }
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies non-identical assertions pass the honesty gate.
///
/// Inputs:
/// - A temporary std test file with one real assertion against different
///   syntax expressions.
///
/// Output:
/// - Success summary with one checked file and one checked test.
///
/// Transformation:
/// - Prevents the gate from rejecting ordinary executable assertions.
#[test]
fn std_test_honesty_accepts_non_identity_assertions() {
    let root = make_quality_temp_dir("std_test_honesty_real_assertion");
    write_file(
        &root,
        "std/sample/RealTest.terl",
        r#"
module sample.RealTest.

@test
pub addition_is_stable(): Bool ->
    std.test.Test.assert_equal(3, 1 + 2).
"#,
    );

    let summary = run_std_test_honesty_for_dir(&root, Path::new("std")).expect("honest tests");

    assert_eq!(summary.checked_file_count, 1);
    assert_eq!(summary.checked_test_count, 1);
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies table-driven fake-test shapes are rejected.
///
/// Inputs:
/// - A temporary std test file with empty tables, duplicate row names,
///   literal-true callbacks, and identity assertions inside table callbacks.
///
/// Output:
/// - Diagnostics naming each fake table test function.
///
/// Transformation:
/// - Extends the structured honesty gate to table helper usage without
///   treating `std.test.Test.each` as a magic language form.
#[test]
fn std_test_honesty_rejects_fake_table_test_patterns() {
    let root = make_quality_temp_dir("std_test_honesty_fake_table_patterns");
    write_file(
        &root,
        "std/sample/TableFakeTest.terl",
        r#"
module sample.TableFakeTest.

import std.collections.List.
import std.test.Test.{each, each_result, row}.

@test
pub empty_table_is_fake(): Bool ->
    each(List(), (_value) -> true).

@test
pub literal_true_callback_is_fake(): Bool ->
    each(List(1, 2), (_value) -> true).

@test
pub identity_assertion_callback_is_fake(): Bool ->
    each(List(1, 2), (value) -> std.test.Test.assert_equal(value, value)).

@test
pub identity_assertion_sequence_callback_is_fake(): Bool ->
    each(List(1, 2), (value) -> std.test.Test.assert_equal(value, value); true).

@test
pub assert_false_false_callback_is_fake(): Bool ->
    each(List(1, 2), (_value) -> std.test.Test.assert_false(false)).

@test
pub duplicate_row_name_is_fake(): Bool ->
    each_result(
        List(
            row("same", 1, 2),
            row("same", 2, 4)
        ),
        (value) -> value + value
    ).
"#,
    );

    let diagnostic =
        run_std_test_honesty_for_dir(&root, Path::new("std")).expect_err("fake table tests");

    for name in [
        "empty_table_is_fake",
        "literal_true_callback_is_fake",
        "identity_assertion_callback_is_fake",
        "identity_assertion_sequence_callback_is_fake",
        "assert_false_false_callback_is_fake",
        "duplicate_row_name_is_fake",
    ] {
        assert!(
            diagnostic.contains(name),
            "expected diagnostic for {name}: {diagnostic}"
        );
    }
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies generated binding contracts are not counted as executable tests.
///
/// Inputs:
/// - A temporary generated std test file with an unannotated contract function.
///
/// Output:
/// - Success summary with one checked file and zero executable tests.
///
/// Transformation:
/// - Keeps generated binding-surface contracts outside the std test honesty
///   executable-test contract.
#[test]
fn std_test_honesty_accepts_unannotated_generated_contracts() {
    let root = make_quality_temp_dir("std_test_honesty_generated_contract");
    write_file(
        &root,
        "std/js/dom/DocumentTest.terl",
        r#"
/**
 * @generated true
 * @do-not-edit true
 */
module std.js.Dom.DocumentTest.

pub generated_binding_surface_contract(): Bool ->
    true.

pub get_element_by_id_typechecks(receiver: Document, element_id: JsString): Option[HTMLElement] ->
    receiver.get_element_by_id(element_id).
"#,
    );

    let summary =
        run_std_test_honesty_for_dir(&root, Path::new("std")).expect("generated contract");

    assert_eq!(summary.checked_file_count, 1);
    assert_eq!(summary.checked_test_count, 0);
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies compatibility test suffixes are discovered.
///
/// Inputs:
/// - A temporary std tree containing a legacy `_Test.terl` file.
///
/// Output:
/// - Diagnostic from the fake test inside that file.
///
/// Transformation:
/// - Keeps old test naming visible to the honesty gate while the canonical
///   suffix remains `Test.terl`.
#[test]
fn std_test_honesty_scans_legacy_capital_test_suffix() {
    let root = make_quality_temp_dir("std_test_honesty_legacy_suffix");
    write_file(
        &root,
        "std/sample/Legacy_Test.terl",
        r#"
module sample.LegacyTest.

@test
pub generated_binding_surface_exists(): Bool ->
    true.
"#,
    );

    let diagnostic =
        run_std_test_honesty_for_dir(&root, Path::new("std")).expect_err("legacy fake test");

    assert!(
        diagnostic.contains("Legacy_Test.terl")
            && diagnostic.contains("generated_binding_surface_exists"),
        "expected legacy suffix diagnostic: {diagnostic}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Writes one fixture file, creating parent directories first.
fn write_file(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("fixture path has parent")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}

/// Creates a unique temporary quality-test directory.
fn make_quality_temp_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_quality_{label}_{}_{}",
        std::process::id(),
        stamp
    ));
    fs::create_dir_all(&root).expect("create fixture root");
    root
}
