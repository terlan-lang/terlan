use super::*;

/// Runs a JavaScript-family build expected to fail before artifact emission.
///
/// Inputs:
/// - `fixture_name`: temporary fixture directory and file stem.
/// - `source`: Terlan source text to compile.
/// - `target`: JavaScript-family build target passed to `terlc build`.
///
/// Output:
/// - The selected output directory after the failed build.
///
/// Transformation:
/// - Writes a single source file, invokes the real build command with the
///   selected JS target, asserts failure, and returns paths for no-artifact
///   checks.
fn run_rejected_js_family_build(
    fixture_name: &str,
    source: &str,
    target: &str,
) -> std::path::PathBuf {
    let dir = make_temp_dir(fixture_name);
    let source_path = dir.join(format!("{fixture_name}.terl"));
    let out_dir = dir.join("build");
    fs::write(&source_path, source).expect("failed to write rejected JS source fixture");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_path.display().to_string(),
                "--target".to_string(),
                target.to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::from(1));
    out_dir
}

/// Runs a shared JavaScript build expected to fail before artifact emission.
///
/// Inputs:
/// - `fixture_name`: temporary fixture directory and file stem.
/// - `source`: Terlan source text to compile with `--target js`.
///
/// Output:
/// - The selected output directory after the failed build.
///
/// Transformation:
/// - Delegates to `run_rejected_js_family_build` with the shared JavaScript
///   target spelling used by default `--target js` builds.
fn run_rejected_js_build(fixture_name: &str, source: &str) -> std::path::PathBuf {
    run_rejected_js_family_build(fixture_name, source, "js")
}

/// Verifies JS builds reject VM process imports before artifact emission.
///
/// Inputs:
/// - A source module importing `std.vm.Process`.
///
/// Output:
/// - Test assertion only; build fails and writes no JS module artifact.
///
/// Transformation:
/// - Runs the real JS build command so target-profile import-family rejection
///   is exercised through formal compilation rather than a synthetic CoreIR
///   fixture alone.
#[test]
fn build_command_rejects_vm_process_import_for_js_target() {
    let out_dir = run_rejected_js_build(
        "build_js_reject_vm_process_std",
        "\
module build_js_reject_vm_process_std.

import std.vm.Process.

pub value(): Int ->
    1.
",
    );

    assert!(!out_dir
        .join("js/modules/build_js_reject_vm_process_std.js")
        .exists());
}

/// Verifies JS builds reject native std imports before artifact emission.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
///
/// Output:
/// - Test assertion only; build fails and writes no JS module artifact.
///
/// Transformation:
/// - Runs the real JS build command so native std import-family rejection is
///   exercised through formal compilation before JS backend emission.
#[test]
fn build_command_rejects_native_std_import_for_js_target() {
    let out_dir = run_rejected_js_build(
        "build_js_reject_native_std",
        "\
module build_js_reject_native_std.

import std.native.collections.Vector.

pub value(): Int ->
    1.
",
    );

    assert!(!out_dir
        .join("js/modules/build_js_reject_native_std.js")
        .exists());
}

/// Verifies JavaScript builds reject Postgres std imports before artifact
/// emission.
///
/// Inputs:
/// - A source module importing `std.db.Postgres` and calling `Postgres.connect`.
///
/// Output:
/// - Test assertion only; build fails before producing JavaScript artifacts.
///
/// Transformation:
/// - Runs the real JS build command so database std APIs cannot enter the
///   browser/shared JS backend before an explicit native bridge contract
///   exists for Postgres.
#[test]
fn build_command_rejects_postgres_std_import_for_js_target() {
    let out_dir = run_rejected_js_build(
        "build_js_reject_postgres_std",
        "\
module build_js_reject_postgres_std.

import std.db.Postgres.
import type std.db.Postgres.Config.
import type std.db.Postgres.Pool.
import type std.core.Error.Error.
import type std.core.Result.Result.

pub connect(config: Config): Result[Pool, Error] ->
    Postgres.connect(config).
",
    );

    assert!(!out_dir
        .join("js/modules/build_js_reject_postgres_std.js")
        .exists());
}

/// Verifies browser DOM bindings are rejected by the shared JS profile.
///
/// Inputs:
/// - A source module importing `std.js.Dom.Document`.
///
/// Output:
/// - Test assertion only; shared JS build fails and writes no artifact.
///
/// Transformation:
/// - Runs the real build command with `--target js.shared` so browser-only
///   generated DOM bindings cannot leak into shared JavaScript output.
#[test]
fn build_command_rejects_browser_dom_import_for_shared_js_target() {
    let out_dir = run_rejected_js_family_build(
        "build_js_shared_reject_dom_std",
        "\
module build_js_shared_reject_dom_std.

import type std.js.Dom.Document.Document.

pub accepts(value: Document): Document ->
    value.
",
        "js.shared",
    );

    assert!(!out_dir
        .join("js/modules/build_js_shared_reject_dom_std.js")
        .exists());
}

/// Verifies JavaScript builds reject function-head destructuring before artifact
/// emission.
///
/// Inputs:
/// - A source module using a typed tuple pattern in a public function head.
///
/// Output:
/// - Test assertion only; shared JS build fails and writes no artifact.
///
/// Transformation:
/// - Runs the real build command with `--target js` so function-head pattern
///   support remains target-explicit instead of silently lowering to a JS
///   function with changed match semantics.
#[test]
fn build_command_rejects_function_head_pattern_for_js_target() {
    let out_dir = run_rejected_js_build(
        "build_js_reject_function_head_pattern",
        "\
module build_js_reject_function_head_pattern.

pub add({left, right}: {Int, Int}): Int ->
    left + right.
",
    );

    assert!(!out_dir
        .join("js/modules/build_js_reject_function_head_pattern.js")
        .exists());
}
