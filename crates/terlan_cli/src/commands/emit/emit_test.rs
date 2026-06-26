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
        "terlan_emit_command_{name}_{}_{}",
        std::process::id(),
        now
    ))
}

/// Verifies `emit` auto-emits SafeNative artifacts for compiler-native std.
///
/// Inputs:
/// - Real `std/data/json.terl` source path.
///
/// Output:
/// - Exit-code and filesystem assertions.
///
/// Transformation:
/// - Runs the normal `emit` command and checks that compiler outputs,
///   derived SafeNative artifacts, and the stable BEAM not-loaded worker
///   reply surface are written together.
#[test]
fn run_emit_writes_compiler_native_std_artifacts() {
    let out_dir = temp_output_dir("std_json");
    let source_path = format!("{}/../../std/data/json.terl", env!("CARGO_MANIFEST_DIR"));
    let exit = run(
        CliCommand {
            verb: Some("emit".to_string()),
            args: vec![source_path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(out_dir.join("std.data.Json.typi").exists());
    assert!(out_dir.join("std.data.Json.safe_native.json").exists());
    assert!(out_dir.join("std_data_json.erl").exists());
    let erl_loader_path = out_dir.join("std_data_json_safe_native.erl");
    assert!(erl_loader_path.exists());
    let erl_loader =
        fs::read_to_string(&erl_loader_path).expect("read generated safe native erl loader");
    assert!(erl_loader.contains("safe_native.not_loaded"));
    assert!(erl_loader.contains("safe_native_not_loaded_error() ->"));
    assert!(erl_loader.contains("{safe_native_reply, RequestId, {error, Error}, 0}"));
    assert!(out_dir
        .join("std_data_json_safe_native.safe_native.rs")
        .exists());

    fs::remove_dir_all(out_dir).expect("remove emit command output");
}

/// Verifies `emit` writes the SQL runtime helper for typed SQL forms.
///
/// Inputs:
/// - A temporary source module containing a ready `sql[UserRow] { ... }`
///   expression.
///
/// Output:
/// - Test passes when the generated Erlang module and `terlan_sql_runtime.erl`
///   are written to the emit output directory.
///
/// Transformation:
/// - Runs the normal single-file emit path and proves CoreIR SQL runtime
///   discovery is shared with build output rather than being build-only.
#[test]
fn run_emit_writes_sql_runtime_for_typed_sql_forms() {
    let root_dir = temp_output_dir("typed_sql_root");
    let out_dir = temp_output_dir("typed_sql_out");
    fs::create_dir_all(&root_dir).expect("create typed SQL fixture directory");
    let source_path = root_dir.join("SqlPage.terl");
    fs::write(
        &source_path,
        "\
module app.SqlPage.\n\
\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
\n\
pub find_user(id: Int): Dynamic ->\n\
sql[UserRow] {SELECT id FROM users WHERE id = ${id} LIMIT 1}.\n",
    )
    .expect("write typed SQL emit fixture");

    let exit = run(
        CliCommand {
            verb: Some("emit".to_string()),
            args: vec![source_path.display().to_string()],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("app_sqlpage.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("terlan_sql_runtime:query_one("),
        "SQL emit output should call the SQL runtime boundary: {}",
        erl_source
    );
    let runtime_source = out_dir.join("terlan_sql_runtime.erl");
    assert!(
        runtime_source.exists(),
        "typed SQL emit should write terlan_sql_runtime.erl"
    );
    let runtime_text = fs::read_to_string(&runtime_source).expect("read emitted SQL runtime");
    assert!(runtime_text.contains("-module(terlan_sql_runtime)."));

    fs::remove_dir_all(root_dir).expect("remove typed SQL fixture root");
    fs::remove_dir_all(out_dir).expect("remove typed SQL emit output");
}
