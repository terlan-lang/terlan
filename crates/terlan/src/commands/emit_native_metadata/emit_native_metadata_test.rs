use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a unique temporary directory for command tests.
///
/// Inputs:
/// - `name`: stable test label included in the directory name.
///
/// Output:
/// - Filesystem path that does not exist before the test uses it.
///
/// Transformation:
/// - Combines process id and current timestamp so parallel test execution
///   does not reuse output directories.
fn temp_output_dir(name: &str) -> std::path::PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "terlan_emit_native_metadata_{name}_{}_{}",
        std::process::id(),
        now
    ))
}

/// Verifies the CLI command emits artifacts for compiler-native std files.
///
/// Inputs:
/// - Real `std/data/Json.terl` source path.
///
/// Output:
/// - Exit-code and filesystem assertions.
///
/// Transformation:
/// - Runs the command through its public module entry point and checks the
///   generated metadata and Rust skeleton filenames.
#[test]
fn run_emits_compiler_native_std_json_artifacts() {
    let out_dir = temp_output_dir("std_json");
    let source_path = format!("{}/../../std/data/Json.terl", env!("CARGO_MANIFEST_DIR"));
    let exit = run(
        CliCommand {
            verb: Some("emit-native-metadata".to_string()),
            args: vec![source_path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(out_dir.join("std.data.Json.native_boundary.json").exists());
    assert!(!out_dir.join("std_data_json_native_boundary.erl").exists());
    assert!(out_dir
        .join("std_data_json_native_boundary.native_boundary.rs")
        .exists());

    fs::remove_dir_all(out_dir).expect("remove native metadata command output");
}
