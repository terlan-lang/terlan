use super::*;

/// Verifies old std runtime VM fixtures cannot use the removed public
/// Vm target.
///
/// Inputs:
/// - A project importing a `std.vm` module.
/// - A build command with the removed `--target erlang` spelling.
///
/// Output:
/// - Exit code `2`.
///
/// Transformation:
/// - Keeps the former VM-backed std runtime test surface as a migration
///   boundary check so 0.0.7 cannot accidentally re-open Vm as a public
///   build target.
#[test]
fn build_command_rejects_erlang_target_for_std_vm_runtime_fixture() {
    let dir = make_temp_dir("directory_project_std_vm_erlang_target_rejected");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.vm.Task.\n\
import type std.vm.Task.Task.\n\
\n\
pub stop(task: Task[Int]): Unit ->\n\
task.cancel().\n\
\n\
pub main(): Unit ->\n\
Unit.\n",
    )
    .expect("failed to write std.vm fixture");

    let state = CliState {
        out_dir,
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

    assert_eq!(status, ExitCode::from(2));
}
