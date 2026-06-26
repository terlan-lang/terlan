use super::*;

/// Runs a build expected to fail before artifact emission.
///
/// Inputs:
/// - `fixture_name`: temporary fixture directory and file stem.
/// - `source`: Terlan source text to compile.
/// - `target`: build target passed to `terlc build`.
///
/// Output:
/// - The selected output directory after the failed build.
///
/// Transformation:
/// - Writes a single source file, invokes the real build command with the
///   selected target, asserts failure, and returns paths for no-artifact
///   checks.
fn run_rejected_build(fixture_name: &str, source: &str, target: &str) -> std::path::PathBuf {
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

/// Runs a JS build expected to fail before artifact emission.
///
/// Inputs:
/// - `fixture_name`: temporary fixture directory and file stem.
/// - `source`: Terlan source text to compile with `--target js`.
///
/// Output:
/// - The selected output directory after the failed build.
///
/// Transformation:
/// - Delegates to `run_rejected_build` with the shared JavaScript target
///   spelling used by default `--target js` builds.
fn run_rejected_js_build(fixture_name: &str, source: &str) -> std::path::PathBuf {
    run_rejected_build(fixture_name, source, "js")
}

/// Verifies Erlang builds reject native package modules.
///
/// Inputs:
/// - A source path and source text declaring `module std.native...`.
///
/// Output:
/// - Test assertion over the stable target-capability diagnostic.
///
/// Transformation:
/// - Exercises the pre-lowering build boundary so native package source cannot
///   accidentally enter Erlang emission.
#[test]
fn reject_erlang_native_package_source_rejects_native_module() {
    let err = reject_erlang_native_package_source(
        "src/std/native/polars/DataFrame.terl",
        "module std.native.polars.DataFrame.\n",
    )
    .expect_err("native module should fail");

    assert!(err.contains("cannot compile native package module"));
    assert!(err.contains("require the Rust/native target capability"));
}

/// Verifies Erlang builds allow native package imports for profile validation.
///
/// Inputs:
/// - A source path and source text importing `std.native...`.
///
/// Output:
/// - Test assertion that raw source preflight accepts the import.
///
/// Transformation:
/// - Exercises the pre-lowering build boundary so imports can proceed to the
///   formal target-profile gate, where bridge-backed native APIs are admitted
///   or rejected by target capability.
#[test]
fn reject_erlang_native_package_source_allows_native_import() {
    let result = reject_erlang_native_package_source(
        "src/app/Main.terl",
        "module app.Main.\n\nimport std.native.polars.DataFrame.\n",
    );

    assert!(result.is_ok());
}

/// Verifies Erlang source preflight allows native vector imports.
///
/// Inputs:
/// - A source path and source text importing `std.native.collections.Vector`.
///
/// Output:
/// - Test assertion that raw source preflight accepts the import.
///
/// Transformation:
/// - Leaves the `std.native.collections.Vector` decision to formal
///   target-profile validation and SafeNative bridge lowering.
#[test]
fn reject_erlang_native_package_source_allows_native_vector_import() {
    let result = reject_erlang_native_package_source(
        "src/app/Main.terl",
        "module app.Main.\n\nimport std.native.collections.Vector.\n",
    );

    assert!(result.is_ok());
}

/// Verifies only Erlang-compatible target profiles can use the Erlang backend.
///
/// Inputs:
/// - Built-in target-profile enum variants for Erlang, A0 Erlang, and portable
///   CoreIR v0.
///
/// Output:
/// - Test assertion only; Erlang profiles must be accepted and CoreV0 must be
///   rejected.
///
/// Transformation:
/// - Calls the target-profile predicate directly so backend capability gating
///   is validated independently from filesystem build setup.
#[test]
fn target_profile_supports_erlang_profiles_only() {
    assert!(target_profile_supports_erlang_backend(
        TargetProfile::Erlang
    ));
    assert!(target_profile_supports_erlang_backend(
        TargetProfile::A0Erlang
    ));
    assert!(!target_profile_supports_erlang_backend(
        TargetProfile::CoreV0
    ));
}

/// Verifies JS builds reject BEAM std imports before artifact emission.
///
/// Inputs:
/// - A source module importing `std.beam.Process`.
///
/// Output:
/// - Test assertion only; build fails and writes no JS module artifact.
///
/// Transformation:
/// - Runs the real JS build command so target-profile import-family rejection
///   is exercised through formal compilation rather than a synthetic CoreIR
///   fixture alone.
#[test]
fn build_command_rejects_beam_std_import_for_js_target() {
    let out_dir = run_rejected_js_build(
        "build_js_reject_beam_std",
        "\
module build_js_reject_beam_std.

import std.beam.Process.

pub value(): Int ->
    1.
",
    );

    assert!(!out_dir
        .join("js/modules/build_js_reject_beam_std.js")
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

/// Verifies Erlang builds route native vector constructor shorthand to a bridge.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A function that uses `Vector("Alice")` constructor shorthand.
///
/// Output:
/// - Test assertion only; build succeeds and writes bridge-backed Erlang
///   artifacts.
///
/// Transformation:
/// - Runs the real Erlang build command so native Vector construction and
///   indexing can enter BEAM builds only through the SafeNative bridge module.
#[test]
fn build_command_compiles_native_vector_constructor_for_erlang_target() {
    let dir = make_temp_dir("build_erlang_native_vector_constructor");
    let source_path = dir.join("build_erlang_native_vector_constructor.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "\
module build_erlang_native_vector_constructor.

import std.native.collections.Vector.

pub first(): String ->
    let users = Vector(\"Alice\", \"Bob\");
    users[0].
",
    )
    .expect("failed to write native vector fixture");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_path.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::from(0));
    let erl = fs::read_to_string(out_dir.join("src/build_erlang_native_vector_constructor.erl"))
        .expect("expected native vector fixture Erlang source");
    assert!(erl.contains("std_native_collections_vector_safe_native:from_list"));
    assert!(erl.contains("std_native_collections_vector_safe_native:get_at"));
    assert!(out_dir
        .join("src/std_native_collections_vector_safe_native.erl")
        .exists());
    assert!(out_dir
        .join("ebin/std_native_collections_vector_safe_native.beam")
        .exists());
}

/// Verifies Erlang builds reject JavaScript std imports before artifact emission.
///
/// Inputs:
/// - A source module importing `std.js.String`.
///
/// Output:
/// - Test assertion only; build fails before producing Erlang artifacts.
///
/// Transformation:
/// - Runs the real build command so JavaScript std import-family rejection is
///   exercised through formal compilation for the default release backend.
#[test]
fn build_command_rejects_js_std_import_for_erlang_target() {
    let out_dir = run_rejected_build(
        "build_erlang_reject_js_std",
        "\
module build_erlang_reject_js_std.

import type std.js.String.JsString.

pub accepts(value: JsString): JsString ->
    value.
",
        "erlang",
    );

    assert!(!out_dir.join("src/build_erlang_reject_js_std.erl").exists());
}

/// Verifies pure Erlang builds reject Postgres std imports before artifact
/// emission.
///
/// Inputs:
/// - A source module importing `std.db.Postgres` and calling `Postgres.connect`.
///
/// Output:
/// - Test assertion only; build fails before producing Erlang artifacts.
///
/// Transformation:
/// - Runs the real build command with native interop disabled so Rust-backed
///   database std import-family rejection is exercised before backend
///   emission.
#[test]
fn build_command_rejects_postgres_std_import_for_pure_erlang_target() {
    let fixture_name = "build_erlang_pure_reject_postgres_std";
    let dir = make_temp_dir(fixture_name);
    let source_path = dir.join(format!("{fixture_name}.terl"));
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "\
module build_erlang_pure_reject_postgres_std.

import std.db.Postgres.
import type std.db.Postgres.Config.
import type std.db.Postgres.Pool.
import type std.core.Error.Error.
import type std.core.Result.Result.

pub connect(config: Config): Result[Pool, Error] ->
    Postgres.connect(config).
",
    )
    .expect("failed to write pure rejected Postgres source fixture");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_path.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            native_policy: NativePolicy::Pure,
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir
        .join("src/build_erlang_pure_reject_postgres_std.erl")
        .exists());
}

/// Verifies default Erlang builds admit Postgres std imports through the
/// SafeNative std bridge gate.
///
/// Inputs:
/// - A source module importing `std.db.Postgres` and calling `Postgres.connect`.
///
/// Output:
/// - Test assertion that the app module artifact is emitted.
///
/// Transformation:
/// - Exercises the real build path with the default `SafeNativeOptional`
///   policy so Postgres remains available to BEAM applications that opt into
///   the maintained Rust adapter.
#[test]
fn build_command_accepts_postgres_std_import_for_default_erlang_target() {
    let fixture_name = "build_erlang_accept_postgres_std";
    let dir = make_temp_dir(fixture_name);
    let source_path = dir.join(format!("{fixture_name}.terl"));
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "\
module build_erlang_accept_postgres_std.

import std.db.Postgres.
import type std.db.Postgres.Config.
import type std.db.Postgres.Pool.
import type std.core.Error.Error.
import type std.core.Result.Result.

pub connect(config: Config): Result[Pool, Error] ->
    Postgres.connect(config).
",
    )
    .expect("failed to write accepted Postgres source fixture");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_path.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir
        .join("src/build_erlang_accept_postgres_std.erl")
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
    let out_dir = run_rejected_build(
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
