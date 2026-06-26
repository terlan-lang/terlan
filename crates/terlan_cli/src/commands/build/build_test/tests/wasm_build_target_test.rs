use super::*;

/// Verifies reserved Wasm build targets are not accepted by package builds yet.
///
/// Inputs:
/// - `terlc build --target wasm.core` over a minimal source file.
///
/// Output:
/// - Exit code `2` from argument validation.
///
/// Transformation:
/// - Keeps the reserved target family visible to diagnostics without letting
///   the build command imply Wasm emission support before the backend exists.
#[test]
fn build_command_rejects_reserved_wasm_target() {
    let dir = make_temp_dir("build_command_reserved_wasm_target");
    let source = dir.join("Main.terl");
    fs::write(&source, "module Main.\n\npub main(): Unit ->\n    Unit.\n")
        .expect("write source fixture");

    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source.display().to_string(),
            "--target".to_string(),
            "wasm.core".to_string(),
        ],
    };

    assert_eq!(run(cmd, CliState::default()), ExitCode::from(2));
}

/// Verifies reserved project Wasm artifacts stop before Erlang build emission.
///
/// Inputs:
/// - Parsed manifest selecting `wasm-browser`.
///
/// Output:
/// - Test assertion only; artifact dispatch must produce a reserved-family
///   diagnostic instead of falling through to the Erlang backend.
///
/// Transformation:
/// - Exercises project artifact dispatch without scanning source roots.
#[test]
fn wasm_build_target_rejects_reserved_wasm_project_artifact() {
    let manifest = project_manifest::parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasm-browser\"\n\n[target.wasm]\nprofile = \"browser\"\n",
        std::path::Path::new("terlan.toml"),
    )
    .expect("manifest should parse");

    let err = reserved_project_artifact_build_error(&manifest)
        .expect("wasm project artifact should be reserved");

    assert!(err.contains("artifact `wasm-browser`"));
    assert!(err.contains("reserved for the Wasm target family"));
}

/// Verifies reserved project WASI artifacts stop before Erlang build emission.
///
/// Inputs:
/// - Parsed manifest selecting `wasi-http`.
///
/// Output:
/// - Test assertion only; artifact dispatch must produce a reserved-family
///   diagnostic instead of falling through to the Erlang backend.
///
/// Transformation:
/// - Exercises project artifact dispatch without scanning source roots.
#[test]
fn wasm_build_target_rejects_reserved_wasi_project_artifact() {
    let manifest = project_manifest::parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasi-http\"\n\n[target.wasi]\nprofile = \"http\"\n",
        std::path::Path::new("terlan.toml"),
    )
    .expect("manifest should parse");

    let err = reserved_project_artifact_build_error(&manifest)
        .expect("wasi project artifact should be reserved");

    assert!(err.contains("artifact `wasi-http`"));
    assert!(err.contains("reserved for the WASI target family"));
}
