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
/// - Real `std/data/json.terl` source path.
///
/// Output:
/// - Exit-code and filesystem assertions.
///
/// Transformation:
/// - Runs the command through its public module entry point and checks the
///   generated metadata, Erlang loader, Rust skeleton filenames, and
///   stable BEAM not-loaded worker reply surface.
#[test]
fn run_emits_compiler_native_std_json_artifacts() {
    let out_dir = temp_output_dir("std_json");
    let source_path = format!("{}/../../std/data/json.terl", env!("CARGO_MANIFEST_DIR"));
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
    assert!(out_dir.join("std.data.Json.safe_native.json").exists());
    let erl_loader_path = out_dir.join("std_data_json_safe_native.erl");
    assert!(erl_loader_path.exists());
    let erl_loader =
        fs::read_to_string(&erl_loader_path).expect("read generated safe native erl loader");
    assert!(erl_loader.contains("safe_native.not_loaded"));
    assert!(erl_loader
        .contains("{safe_native_reply, RequestId, {error, safe_native_not_loaded_error()}, 0}"));
    assert!(out_dir
        .join("std_data_json_safe_native.safe_native.rs")
        .exists());

    fs::remove_dir_all(out_dir).expect("remove native metadata command output");
}
