use super::*;

/// Verifies typed SQL forms cause Erlang builds to emit the SQL runtime stub.
///
/// Inputs:
/// - A project with a local row struct and a function body using
///   `sql[UserRow] { ... }`.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits and compiles
///   `terlan_sql_runtime`.
///
/// Transformation:
/// - Exercises the formal pipeline through CoreIR SQL payload detection, then
///   proves the build layer writes the runtime boundary module required by the
///   generated wrapper call without importing the future Postgres std module.
#[test]
fn build_command_emits_sql_runtime_for_typed_sql_forms() {
    let dir = make_temp_dir("directory_project_typed_sql_runtime");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
\n\
pub find_user(id: Int): Dynamic ->\n\
sql[UserRow] {SELECT id FROM users WHERE id = ${id} LIMIT 1}.\n\
\n\
pub main(): Unit ->\n\
println(\"ok\").\n",
    )
    .expect("failed to write typed SQL fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("terlan_sql_runtime:query_one("),
        "SQL wrapper call should lower to runtime boundary: {}",
        erl_source
    );
    let runtime_source = out_dir.join("src/terlan_sql_runtime.erl");
    assert!(
        runtime_source.exists(),
        "typed SQL build should emit terlan_sql_runtime.erl"
    );
    let runtime_beam = out_dir.join("ebin/terlan_sql_runtime.beam");
    assert!(
        runtime_beam.exists(),
        "typed SQL build should compile terlan_sql_runtime.beam"
    );
}
