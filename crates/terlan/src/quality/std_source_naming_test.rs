use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::run_std_source_naming;

/// Verifies matching hand-authored std filenames pass the convention gate.
///
/// Inputs:
/// - Temporary std source `std/core/Float.terl`.
///
/// Output:
/// - Successful naming summary for one checked source.
///
/// Transformation:
/// - Runs the repository quality gate against a minimal fixture tree.
#[test]
fn std_source_naming_accepts_matching_module_filename() {
    let repo = temp_repo("std_source_naming_accepts");
    write(
        &repo,
        "std/core/Float.terl",
        "module std.core.Float.\n\npub value(): Int ->\n    1.\n",
    );

    let summary = run_std_source_naming(&repo).expect("matching std source should pass");

    assert_eq!(summary.checked_source_count, 1);
}

/// Verifies lowercase implementation filenames are rejected for Pascal modules.
///
/// Inputs:
/// - Temporary std source `std/core/float.terl` declaring `std.core.Float`.
///
/// Output:
/// - Stable diagnostic naming the expected filename.
///
/// Transformation:
/// - Runs the repository quality gate and checks the convention failure.
#[test]
fn std_source_naming_rejects_lowercase_module_filename() {
    let repo = temp_repo("std_source_naming_rejects_lowercase");
    write(
        &repo,
        "std/core/float.terl",
        "module std.core.Float.\n\npub value(): Int ->\n    1.\n",
    );

    let error = run_std_source_naming(&repo).expect_err("lowercase source should fail");

    assert!(error.contains("std/core/float.terl"));
    assert!(error.contains("expected `Float.terl`"));
}

/// Verifies lowercase std module declarations are rejected even when the
/// filename matches.
///
/// Inputs:
/// - Temporary std source `std/core/float.terl` declaring `std.core.float`.
///
/// Output:
/// - Stable diagnostic naming the lowercase final module segment.
///
/// Transformation:
/// - Locks the UpperCamel-style std module convention so source naming cannot
///   drift back to lowercase module files.
#[test]
fn std_source_naming_rejects_lowercase_module_segment() {
    let repo = temp_repo("std_source_naming_rejects_lowercase_segment");
    write(
        &repo,
        "std/core/float.terl",
        "module std.core.float.\n\npub value(): Int ->\n    1.\n",
    );

    let error = run_std_source_naming(&repo).expect_err("lowercase module should fail");

    assert!(error.contains("std/core/float.terl"));
    assert!(error.contains("final segment `float`"));
}

/// Verifies generated foreign module names may keep uppercase-leading
/// underscore forms.
///
/// Inputs:
/// - Temporary generated-style source `std/js/ANGLE_instanced_arrays.terl`.
///
/// Output:
/// - Successful naming summary for one checked source.
///
/// Transformation:
/// - Preserves generated Web/API names while still enforcing the uppercase
///   leading module convention.
#[test]
fn std_source_naming_accepts_uppercase_leading_generated_foreign_segment() {
    let repo = temp_repo("std_source_naming_accepts_foreign_segment");
    write(
        &repo,
        "std/js/ANGLE_instanced_arrays.terl",
        "module std.js.ANGLE_instanced_arrays.\n",
    );

    let summary = run_std_source_naming(&repo).expect("foreign std source should pass");

    assert_eq!(summary.checked_source_count, 1);
}

/// Verifies generated JavaScript bindings follow the std filename convention.
///
/// Inputs:
/// - Temporary generated-style source under `std/js` with an old snake_case
///   filename.
///
/// Output:
/// - Stable diagnostic naming the expected PascalCase filename.
///
/// Transformation:
/// - Confirms generated bindings are validated by the same module-to-filename
///   convention as hand-authored std sources.
#[test]
fn std_source_naming_rejects_generated_js_snake_case_filenames() {
    let repo = temp_repo("std_source_naming_rejects_js_snake_case");
    write(
        &repo,
        "std/js/array_buffer.terl",
        "module std.js.ArrayBuffer.\n",
    );

    let error = run_std_source_naming(&repo).expect_err("generated js filename should fail");

    assert!(error.contains("std/js/array_buffer.terl"));
    assert!(error.contains("expected `ArrayBuffer.terl`"));
}

/// Verifies mistyped `.tert` source extensions fail explicitly.
///
/// Inputs:
/// - Temporary std source with a `.tert` extension.
///
/// Output:
/// - Stable diagnostic naming the unsupported extension.
///
/// Transformation:
/// - Runs the repository quality gate and checks extension spelling feedback.
#[test]
fn std_source_naming_rejects_tert_extension() {
    let repo = temp_repo("std_source_naming_rejects_tert");
    write(&repo, "std/core/Float.tert", "module std.core.Float.\n");

    let error = run_std_source_naming(&repo).expect_err("tert source should fail");

    assert!(error.contains("std/core/Float.tert"));
    assert!(error.contains("unsupported Terlan source extension `.tert`"));
}

/// Creates an isolated temporary repository fixture directory.
///
/// Inputs:
/// - `name`: fixture prefix.
///
/// Output:
/// - Empty temporary directory path.
///
/// Transformation:
/// - Uses the process id and current timestamp to avoid collisions.
fn temp_repo(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(&path).expect("create temp repo");
    path
}

/// Writes a fixture file under a temporary repository.
///
/// Inputs:
/// - `root`: temporary repository root.
/// - `relative`: repository-relative fixture path.
/// - `text`: fixture contents.
///
/// Output:
/// - File written on disk.
///
/// Transformation:
/// - Creates parent directories before writing the fixture text.
fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture should have parent")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}
