use super::*;

/// Verifies a project image links imported standard-library implementation modules.
///
/// Inputs:
/// - A manifest-backed VM application whose entrypoint calls `std.data.Json`.
///
/// Output:
/// - A native image whose `main/0` reaches the imported JSON implementation.
///
/// Transformation:
/// - Exercises directory/project compilation rather than the standalone-file path so
///   imported checked-in standard-library sources must be added to the application
///   closure before NativeIR reachability analysis.
#[test]
fn project_vm_build_links_imported_std_source_closure() {
    let dir = make_temp_dir("project_vm_std_source_closure");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("create project source directory");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write project manifest");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\nimport std.collections.Map.\nimport std.data.Json.\n\npub main(): Unit ->\n    let value = Json.object();\n    let _put = value.put(\"linked\", Json.bool(true));\n    Unit.\n",
    )
    .expect("write application entrypoint");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                project_dir.display().to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        state,
    );

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("vm/app_Main.tvm").is_file());
}
