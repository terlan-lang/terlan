use super::*;

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

/// Verifies Erlang builds reject native package imports.
///
/// Inputs:
/// - A source path and source text importing `std.native...`.
///
/// Output:
/// - Test assertion over the stable target-capability diagnostic.
///
/// Transformation:
/// - Exercises the pre-lowering build boundary so consumers of native packages
///   fail before unresolved import or backend diagnostics.
#[test]
fn reject_erlang_native_package_source_rejects_native_import() {
    let err = reject_erlang_native_package_source(
        "src/app/Main.terl",
        "module app.Main.\n\nimport std.native.polars.DataFrame.\n",
    )
    .expect_err("native import should fail");

    assert!(err.contains("cannot import native package"));
    assert!(err.contains("require the Rust/native target capability"));
}

/// Verifies Erlang builds reject native vector imports.
///
/// Inputs:
/// - A source path and source text importing `std.native.collections.Vector`.
///
/// Output:
/// - Test assertion over the stable target-capability diagnostic.
///
/// Transformation:
/// - Locks the release-owned native vector std contract behind the same
///   Rust/native target gate as other `std.native.*` packages.
#[test]
fn reject_erlang_native_package_source_rejects_native_vector_import() {
    let err = reject_erlang_native_package_source(
        "src/app/Main.terl",
        "module app.Main.\n\nimport std.native.collections.Vector.\n",
    )
    .expect_err("native vector import should fail");

    assert!(err.contains("cannot import native package"));
    assert!(err.contains("require the Rust/native target capability"));
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
