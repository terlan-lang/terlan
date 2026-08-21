use super::*;

/// Verifies a native Terlan service launcher targets its bundled HTTP runtime.
///
/// Inputs:
/// - A package-local launcher path.
///
/// Output:
/// - Test passes when the launcher maps the platform port, enables trusted
///   service capabilities by default, and executes the adjacent runtime over
///   the packaged web root.
///
/// Transformation:
/// - Writes the portable service entrypoint without requiring a compiler or a
///   live HTTP listener.
#[test]
fn vm_service_launcher_executes_bundled_service_runtime() {
    let dir = make_temp_dir("vm_service_launcher");
    let launcher = dir.join("bin/app");

    write_vm_service_launcher(&launcher, false).expect("write VM service launcher");

    let contents = fs::read_to_string(&launcher).expect("read VM service launcher");
    if cfg!(windows) {
        assert!(contents.contains("TERLAN_SERVE_PORT=%PORT%"));
        assert!(contents.contains("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES=1"));
        assert!(contents.contains("terlan-serve-runtime.exe"));
        assert!(contents.contains("..\\web"));
    } else {
        assert!(contents.contains("export TERLAN_SERVE_PORT=$PORT"));
        assert!(contents.contains(
            "export TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES=${TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES:-1}"
        ));
        assert!(contents.contains("exec \"$SCRIPT_DIR/terlan-serve-runtime\""));
        assert!(contents.contains("\"$SCRIPT_DIR/../web\""));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(&launcher)
            .expect("service launcher metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "service launcher should be executable");
    }
}

/// Verifies executable VM artifact packages require an executable entrypoint.
///
/// Inputs:
/// - A project manifest selecting the `terlan-vm` artifact.
/// - A package-rooted `app.Main` module that lacks `main/0`.
///
/// Output:
/// - Test passes when the VM artifact build rejects the package before writing
///   a user-facing executable launcher or package metadata.
///
/// Transformation:
/// - Runs the manifest project build and proves executable packages cannot
///   silently produce a native image without an executable export.
#[test]
fn build_command_rejects_project_manifest_without_main_entrypoint_for_vm_artifact() {
    let dir = make_temp_dir("directory_project_manifest_missing_entrypoint");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write manifest source-root module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(out_dir.join("vm/app_Main.tvm").exists());
    assert!(!out_dir.join("bin/app").exists());
    assert!(!out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists());
}

/// Verifies executable VM artifact packages emit a runnable launcher contract.
///
/// Inputs:
/// - A project manifest selecting the default executable `terlan-vm` artifact.
/// - A package-rooted `app.Main.main/0` entrypoint.
///
/// Output:
/// - Test passes when the build emits the VM artifact, `_build/bin/app`
///   launcher, and package metadata pointing at that launcher.
///
/// Transformation:
/// - Exercises the package build contract that prevents `terlc build` from
///   succeeding without producing an executable artifact.
#[test]
fn build_command_emits_executable_launcher_for_vm_artifact_package() {
    let dir = make_temp_dir("directory_project_manifest_executable_launcher");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\nprivate_value(): Int ->\n    42.\n\npub main(): Unit ->\n    let value = private_value(); Unit.\n",
    )
    .expect("failed to write manifest source-root module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let image_path = out_dir.join("vm/app_Main.tvm");
    assert!(image_path.exists());
    assert!(!out_dir.join("vm/app_Main.tvm.json").exists());
    let image = fs::read(&image_path).expect("read native package image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let inspection = crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect native package image");
    assert_eq!(
        inspection
            .descriptor
            .exports
            .iter()
            .map(|export| export.name.as_str())
            .collect::<Vec<_>>(),
        vec!["app.Main.main/0"],
        "private native helpers must not cross the image boundary"
    );
    let launcher = out_dir.join("bin/app");
    assert!(launcher.exists());
    assert!(out_dir.join("bin/terlan-vm").exists());
    assert!(out_dir.join("bin/terlan-native-worker").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(&launcher)
            .expect("launcher metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "launcher should be executable");
    }
    let metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
        .expect("read package build metadata");
    let metadata: serde_json::Value =
        serde_json::from_str(&metadata_text).expect("parse package build metadata");
    assert_eq!(metadata["executable"]["path"], "bin/app");
    assert_eq!(metadata["executable"]["image"], "vm/app_Main.tvm");
    assert_eq!(metadata["executable"]["runtime"], "bin/terlan-vm");
    assert_eq!(
        metadata["executable"]["native_worker"],
        "bin/terlan-native-worker"
    );
    let output = std::process::Command::new(&launcher)
        .output()
        .expect("run native package launcher");
    assert!(
        output.status.success(),
        "native package launcher failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Verifies manifest builds lower explicit constructor declarations.
///
/// Inputs:
/// - A manifest-backed `terlan-vm` project.
/// - A package-rooted `app.Main` module with one public constructor and
///   one private constructor used by `main/0`.
///
/// Output:
/// - Test passes when the build emits a VM artifact for the constructor-heavy
///   entrypoint without producing Vm or VM artifacts.
///
/// Transformation:
/// - Compiles explicit constructor declarations through the formal CoreIR
///   build path and proves the VM artifact path accepts public and private
///   constructor use.
#[test]
fn build_command_compiles_project_explicit_constructor_entrypoint() {
    let dir = make_temp_dir("directory_project_explicit_constructor_entrypoint");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
pub type Done = Int.\n\
type Hidden = Int.\n\
\n\
pub constructor Done {\n\
(value: Int): Done -> value\n\
}.\n\
\n\
constructor Hidden {\n\
(value: Int): Hidden -> value\n\
}.\n\
\n\
pub main(): Unit ->\n\
let visible = Done(1); let hidden = Hidden(2); std.io.Console.println(\"constructors ok\").\n",
    )
    .expect("failed to write explicit constructor module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert!(out_dir.join(".terlan/app.Main.typi").exists());
    assert!(!out_dir.join("bin/app").exists());
}

/// Verifies manifest VM builds accept receiver-method dispatch in the VM
/// artifact lane.
///
/// Inputs:
/// - A manifest-backed `terlan-vm` project.
/// - A package-rooted `app.Main` module with a struct, a receiver method,
///   and an executable entrypoint that invokes the method through
///   `receiver.method()`.
///
/// Output:
/// - Test passes when the VM build emits a VM artifact for receiver-method
///   dispatch without producing Vm or BEAM artifacts.
///
/// Transformation:
/// - Runs local receiver-method dispatch through the formal build path and
///   proves the VM artifact path accepts the receiver call shape.
#[test]
fn build_command_compiles_project_receiver_method_entrypoint() {
    let dir = make_temp_dir("directory_project_receiver_method_entrypoint");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
pub struct User {\n\
name: String\n\
}.\n\
\n\
pub constructor User {\n\
(name: String): User -> User(name = name)\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
user.name.\n\
\n\
show(user: User): String ->\n\
user.display_name().\n\
\n\
pub main(): Unit ->\n\
std.io.Console.println(show(User(\"Ada\"))).\n",
    )
    .expect("failed to write receiver-method module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert!(out_dir.join(".terlan/app.Main.typi").exists());
    assert!(!out_dir.join("bin/app").exists());
}
