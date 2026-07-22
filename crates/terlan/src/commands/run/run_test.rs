use super::*;

use crate::support::test_fs::temp_dir as shared_temp_dir;

/// Writes a tiny executable shell script on Unix test platforms.
///
/// Inputs:
/// - `path`: output script path.
/// - `body`: shell script body after the shebang.
///
/// Output:
/// - Executable file at `path`.
///
/// Transformation:
/// - Writes a POSIX shell script and sets user/group/other executable bits so
///   the run command can execute it like a generated launcher.
#[cfg(unix)]
fn write_executable_script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, format!("#!/usr/bin/env sh\n{body}\n")).expect("write script");
    let mut permissions = fs::metadata(path).expect("script metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("set executable bit");
}

/// Writes minimal build metadata pointing at a package launcher.
///
/// Inputs:
/// - `out_dir`: build output directory.
/// - `launcher_path`: launcher path relative to `out_dir`.
///
/// Output:
/// - `terlan-package-build.json` in the output directory.
///
/// Transformation:
/// - Emits only the metadata fields consumed by the run command so tests stay
///   focused on the run/build handoff contract.
fn write_run_metadata(out_dir: &Path, launcher_path: &str) {
    fs::write(
        out_dir.join(BUILD_PACKAGE_METADATA_FILE),
        format!(r#"{{"executable":{{"path":"{launcher_path}"}}}}"#),
    )
    .expect("write run metadata");
}

/// Verifies the run command defaults to the compiler-owned VM target.
///
/// Inputs:
/// - Empty command-local argument vector.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Exercises target validation directly so bare `terlc run` cannot drift
///   away from the compiler-owned VM lane.
#[test]
fn validate_run_args_defaults_to_vm_target() {
    assert_eq!(validate_run_args(&[]), Ok(RunTarget::TerlanVm));
}

/// Verifies the public run target surface is VM-only.
///
/// Inputs:
/// - Command-local `--target terlan-vm` and removed `--target erlang`
///   arguments.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Exercises target validation directly so the removed stock-runtime target
///   cannot re-enter the public run command.
#[test]
fn validate_run_args_accepts_vm_and_rejects_erlang_target() {
    assert_eq!(
        validate_run_args(&["--target".to_string(), "erlang".to_string()]),
        Err("run target `erlang` was removed from the public CLI; use `terlan-vm`".to_string())
    );
    assert_eq!(
        validate_run_args(&["--target".to_string(), "terlan-vm".to_string()]),
        Ok(RunTarget::TerlanVm)
    );
}

/// Verifies removed runtime selectors cannot enter run delegation.
#[test]
fn validate_run_args_rejects_runtime_fallback_selection() {
    assert_eq!(
        validate_run_args(&["--runtime".to_string(), "interpreter".to_string()]),
        Err("unknown run option: --runtime".to_string())
    );
}

/// Verifies bare run commands forward an explicit VM target to build.
///
/// Inputs:
/// - A run command with only a source path.
///
/// Output:
/// - Command arguments containing `--target terlan-vm`.
///
/// Transformation:
/// - Applies the run-to-build handoff helper so build output matches the
///   VM-first run target instead of falling back to build's legacy default.
#[test]
fn build_command_for_run_appends_default_vm_target() {
    let build_cmd = build_command_for_run(
        CliCommand {
            verb: Some("run".to_string()),
            args: vec!["src/Main.terl".to_string()],
        },
        RunTarget::TerlanVm,
    );

    assert_eq!(
        build_cmd.args,
        vec![
            "src/Main.terl".to_string(),
            "--target".to_string(),
            "terlan-vm".to_string()
        ]
    );
}

/// Verifies explicit run targets are not rewritten during build delegation.
///
/// Inputs:
/// - A run command with an explicit Terlan VM target.
///
/// Output:
/// - Command arguments preserved exactly.
///
/// Transformation:
/// - Applies the run-to-build handoff helper and proves explicit user target
///   selection is not duplicated by the default-target appender.
#[test]
fn build_command_for_run_preserves_explicit_target() {
    let build_cmd = build_command_for_run(
        CliCommand {
            verb: Some("run".to_string()),
            args: vec![
                "src/Main.terl".to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        RunTarget::TerlanVm,
    );

    assert_eq!(
        build_cmd.args,
        vec![
            "src/Main.terl".to_string(),
            "--target".to_string(),
            "terlan-vm".to_string()
        ]
    );
}

/// Verifies named project scripts are expanded to concrete source paths before build.
///
/// Inputs:
/// - Temporary project containing one conventional runnable script.
///
/// Output:
/// - Rewritten command whose source argument points at the script file.
///
/// Transformation:
/// - Exercises only the command-local `run script` sugar so normal target
///   validation and build delegation continue to operate on a direct source.
#[test]
fn expand_script_run_command_resolves_convention_script() {
    let temp = shared_temp_dir("run_command", "resolve_convention_script");
    let scripts_dir = temp.join("scripts");
    fs::create_dir_all(&scripts_dir).expect("create scripts dir");
    let script = scripts_dir.join("SeedDatabase.terl");
    fs::write(
        &script,
        "\
module scripts.SeedDatabase.

pub main(): Unit ->
    Unit.
",
    )
    .expect("write script");
    let expanded = expand_script_run_command_in_project(
        CliCommand {
            verb: Some("run".to_string()),
            args: vec!["script".to_string(), "seed_database".to_string()],
        },
        &temp,
    )
    .expect("expand script command");

    assert_eq!(expanded.args, vec![script.to_string_lossy().into_owned()]);
}

/// Verifies unsupported run targets are rejected before build delegation.
///
/// Inputs:
/// - Command-local `--target js` argument.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Exercises target validation directly so unsupported backend execution
///   remains a command-line error instead of a missing-launcher failure.
#[test]
fn validate_run_args_rejects_unsupported_target() {
    assert_eq!(
        validate_run_args(&["--target".to_string(), "js".to_string()]),
        Err("terlc run currently supports --target terlan-vm, got `js`".to_string())
    );
}

/// Verifies `run` refuses JS typed evidence before forcing a VM build target.
///
/// Inputs:
/// - Temporary source file importing `std.js.Promise`.
///
/// Output:
/// - Stable target-inference error.
///
/// Transformation:
/// - Exercises the run preflight directly so `terlc run` cannot append its
///   synthetic VM target and hide a non-VM source requirement.
#[test]
fn run_command_rejects_js_target_evidence_before_build() {
    let temp = shared_temp_dir("run_command", "rejects_js_target_evidence");
    let source = temp.join("Main.terl");
    fs::write(
        &source,
        "\
module app.Main.

import type std.js.Promise.

pub accepts(value: Promise[Int]): Promise[Int] ->
    value.
",
    )
    .expect("write js source");

    let message =
        validate_run_target_evidence(&[source.to_string_lossy().into_owned()], TargetProfile::Vm)
            .expect_err("expected JS run rejection");

    assert!(
        message.contains("source evidence requires `js.shared`"),
        "{message}"
    );
}

/// Verifies explicit non-VM global profiles cannot select `run` execution.
///
/// Inputs:
/// - Target-neutral VM source plus a global JS target profile.
///
/// Output:
/// - Stable VM-only runtime diagnostic.
///
/// Transformation:
/// - Confirms `run` keeps target-neutral code on the VM runtime instead of
///   allowing a non-VM execution target.
#[test]
fn run_command_rejects_explicit_js_profile_for_vm_source() {
    let temp = shared_temp_dir("run_command", "rejects_explicit_js_profile");
    let source = temp.join("Main.terl");
    fs::write(
        &source,
        "\
module app.Main.

pub main(): Int ->
    1.
",
    )
    .expect("write vm source");

    let message = validate_run_target_evidence(
        &[source.to_string_lossy().into_owned()],
        TargetProfile::JsShared,
    )
    .expect_err("expected explicit JS profile conflict");

    assert!(
        message.contains(
            "`terlc run` executes VM programs, but explicit target `js.shared` was requested"
        ),
        "{message}"
    );
}

/// Verifies executable metadata is loaded relative to the output directory.
///
/// Inputs:
/// - Temporary output directory with minimal package metadata.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Confirms the run command resolves the build-recorded launcher path without
///   relying on package names or other build metadata.
#[test]
fn load_executable_path_reads_build_metadata() {
    let temp = shared_temp_dir("run_command", "metadata");
    write_run_metadata(&temp, "bin/app");
    let metadata = load_run_metadata(&temp).expect("load run metadata");

    assert_eq!(
        executable_path_from_metadata(&temp, &metadata).expect("load executable path"),
        temp.join("bin/app")
    );
}

/// Verifies missing executable metadata is reported as a run failure.
///
/// Inputs:
/// - Temporary output directory with metadata that lacks an executable entry.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Ensures `terlc run` rejects non-executable build artifacts with a precise
///   metadata-oriented message.
#[test]
fn load_executable_path_rejects_missing_executable_entry() {
    let temp = shared_temp_dir("run_command", "missing_executable");
    fs::write(
        temp.join(BUILD_PACKAGE_METADATA_FILE),
        r#"{"executable":null}"#,
    )
    .expect("write metadata");

    let metadata = load_run_metadata(&temp).expect("load run metadata");
    let message =
        executable_path_from_metadata(&temp, &metadata).expect_err("expected missing executable");
    assert!(
        message.contains("does not describe an executable package artifact"),
        "{message}"
    );
}

/// Verifies direct source runs select their module artifact despite stale output.
#[test]
fn find_native_image_for_source_ignores_other_native_images() {
    let temp = shared_temp_dir("run_command", "source_vm_artifact");
    let vm_dir = temp.join("vm");
    fs::create_dir_all(&vm_dir).expect("create vm dir");
    let source = temp.join("test.terl");
    fs::write(
        &source,
        "module scripts.test.\n\npub main(): Unit -> Unit.\n",
    )
    .expect("write source");
    fs::write(vm_dir.join("hello_Main.tvm"), "{}").expect("write stale artifact");
    let expected = vm_dir.join("scripts_test.tvm");
    fs::write(&expected, "{}").expect("write script artifact");

    assert_eq!(
        find_native_image_for_source(&temp, &source).expect("find source native image"),
        expected
    );
}

/// Verifies VM source runs reject transitional `.tvm.json` artifacts.
#[test]
fn find_native_image_for_source_rejects_transitional_artifact() {
    let temp = shared_temp_dir("run_command", "source_vm_legacy_artifact");
    let vm_dir = temp.join("vm");
    fs::create_dir_all(&vm_dir).expect("create vm dir");
    let source = temp.join("legacy.terl");
    fs::write(
        &source,
        "module scripts.legacy.\n\npub main(): Unit -> Unit.\n",
    )
    .expect("write source");
    let legacy = vm_dir.join("scripts_legacy.tvm.json");
    fs::write(&legacy, "{}").expect("write legacy artifact");
    let err = find_native_image_for_source(&temp, &source)
        .expect_err("expected legacy artifact rejection");
    assert!(
        err.contains("expected native image"),
        "wrong rejection: {err}"
    );
}

/// Verifies VM artifact execution delegates to the supplied VM runner.
///
/// Inputs:
/// - Temporary output directory with one `.tvm` image.
/// - Fake executable VM runner.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Exercises the same artifact-to-runner handoff used by
///   `terlc run --target terlan-vm` while keeping the test independent of the
///   standalone VM binary.
#[cfg(unix)]
#[test]
fn run_built_native_image_executes_vm_runner() {
    let temp = shared_temp_dir("run_command", "vm_runner");
    let vm_dir = temp.join("vm");
    fs::create_dir_all(&vm_dir).expect("create vm dir");
    let source = temp.join("main.terl");
    fs::write(&source, "module app.main.\n\npub main(): Unit -> Unit.\n").expect("write source");
    fs::write(vm_dir.join("app_main.tvm"), "{}").expect("write artifact");
    let helper = temp.join("artifact-helper");
    write_executable_script(&helper, "exit 0");
    let helper_env = format!("TERLAN_TEST_VM_RUNNER_HELPER_{}", std::process::id());
    fs::write(
        temp.join(BUILD_PACKAGE_METADATA_FILE),
        format!(
            r#"{{"executable":null,"native":{{"rust":null,"rust_dependencies":[],"artifact_environment":[{{"name":"{helper_env}","path":"{}"}}]}}}}"#,
            helper.display()
        ),
    )
    .expect("write VM native metadata");
    let runner = temp.join("terlan-vm");
    write_executable_script(
        &runner,
        &format!(
            r#"test "$1" = "run"
test -f "$2"
test "${helper_env}" = "{}"
exit 0"#,
            helper.display()
        ),
    );
    let state = CliState {
        out_dir: temp.clone(),
        ..CliState::default()
    };

    assert_eq!(
        run_built_native_image_with_runner(&state, &runner, &source).expect("run native image"),
        ExitCode::SUCCESS
    );
}

/// Verifies native helper metadata is converted into child environment.
///
/// Inputs:
/// - Loaded run metadata with a root helper and one local dependency helper.
/// - Existing fake helper executables in conventional Cargo debug locations.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Exercises helper discovery without launching Vm so the run command
///   can set NativeBoundary helper env vars from build metadata.
#[cfg(unix)]
#[test]
fn discover_native_helper_envs_reads_root_and_dependency_helpers() {
    let temp = shared_temp_dir("run_command", "native_helper_envs");
    let root_dir = temp.join("root");
    let dep_dir = temp.join("dep");
    let root_helper = root_dir
        .join("native")
        .join("target")
        .join("debug")
        .join("root-native-boundary");
    let dep_helper = dep_dir
        .join("native")
        .join("target")
        .join("debug")
        .join("dep-native-boundary");
    fs::create_dir_all(root_helper.parent().expect("root helper parent"))
        .expect("create root helper dir");
    fs::create_dir_all(dep_helper.parent().expect("dependency helper parent"))
        .expect("create dependency helper dir");
    write_executable_script(&root_helper, "exit 0");
    write_executable_script(&dep_helper, "exit 0");

    fs::write(
        temp.join(BUILD_PACKAGE_METADATA_FILE),
        format!(
            r#"{{
                "executable":{{"path":"bin/app"}},
                "native":{{
                    "rust":{{
                        "path":"native",
                        "helper":"root-native-boundary",
                        "helper_env":"TERLAN_TEST_ROOT_NATIVE_BOUNDARY_PATH_{}",
                        "package_dir":"{}"
                    }},
                    "rust_dependencies":[
                        {{
                            "package":"dep",
                            "version":"0.0.1",
                            "rust":{{
                                "path":"native",
                                "helper":"dep-native-boundary",
                                "helper_env":"TERLAN_TEST_DEP_NATIVE_BOUNDARY_PATH_{}",
                                "package_dir":"{}"
                            }}
                        }}
                    ]
                }}
            }}"#,
            std::process::id(),
            root_dir.display(),
            std::process::id(),
            dep_dir.display()
        ),
    )
    .expect("write run metadata with native helpers");

    let metadata = load_run_metadata(&temp).expect("load metadata");
    let envs = discover_native_helper_envs(&metadata).expect("discover helper envs");

    assert_eq!(envs.len(), 2);
    assert_eq!(envs[0].1, root_helper);
    assert_eq!(envs[1].1, dep_helper);
}

/// Verifies a cached artifact binding takes precedence over source Cargo metadata.
#[cfg(unix)]
#[test]
fn discover_native_helper_envs_prefers_prebuilt_artifact_binding() {
    let temp = shared_temp_dir("run_command", "artifact_native_helper_env");
    let artifact_helper = temp.join("artifact-helper");
    write_executable_script(&artifact_helper, "exit 0");
    let env_name = format!("TERLAN_TEST_ARTIFACT_NATIVE_PATH_{}", std::process::id());
    fs::write(
        temp.join(BUILD_PACKAGE_METADATA_FILE),
        format!(
            r#"{{
                "executable":{{"path":"bin/app"}},
                "native":{{
                    "artifact_environment":[{{
                        "name":"{env_name}",
                        "path":"{}"
                    }}],
                    "rust_dependencies":[{{
                        "package":"dep",
                        "version":"0.0.1",
                        "rust":{{
                            "path":"missing-native-source",
                            "helper":"missing-helper",
                            "helper_env":"{env_name}",
                            "package_dir":"{}"
                        }}
                    }}]
                }}
            }}"#,
            artifact_helper.display(),
            temp.display()
        ),
    )
    .expect("write artifact run metadata");

    let metadata = load_run_metadata(&temp).expect("load artifact metadata");
    let envs = discover_native_helper_envs(&metadata).expect("discover artifact helper");

    assert_eq!(envs, vec![(env_name, artifact_helper)]);
}

/// Verifies Cargo helper builds include declared native Rust features.
///
/// Inputs:
/// - In-memory helper metadata with a package directory, crate path, helper
///   executable, and feature list.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Exercises the argument builder used by `terlc run` before it invokes
///   Cargo, without depending on a real Rust crate in this unit test.
#[test]
fn native_helper_build_args_include_manifest_bin_and_features() {
    let native = RunRustNativeMetadata {
        path: "native".to_string(),
        helper: "demo-native-boundary".to_string(),
        helper_env: "DEMO_NATIVE_BOUNDARY_PATH".to_string(),
        features: vec!["real-polars".to_string(), "csv".to_string()],
        package_dir: Some("/tmp/demo".to_string()),
        target_dir: None,
    };

    assert_eq!(
        native_helper_build_args(&native),
        vec![
            "build".to_string(),
            "--manifest-path".to_string(),
            "/tmp/demo/native/Cargo.toml".to_string(),
            "--bin".to_string(),
            "demo-native-boundary".to_string(),
            "--features".to_string(),
            "real-polars,csv".to_string(),
        ]
    );
}

/// Verifies the run command executes the launcher recorded by build metadata.
///
/// Inputs:
/// - Temporary output directory with a fake generated launcher.
///
/// Output:
/// - Test assertion success or panic.
///
/// Transformation:
/// - Runs the same metadata-to-launcher path used after a real build while
///   avoiding an Vm compiler dependency in this focused unit test.
#[cfg(unix)]
#[test]
fn run_built_executable_executes_metadata_launcher() {
    let temp = shared_temp_dir("run_command", "launcher");
    let bin_dir = temp.join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    write_executable_script(&bin_dir.join("app"), "exit 0");
    write_run_metadata(&temp, "bin/app");

    let state = CliState {
        out_dir: temp,
        ..CliState::default()
    };

    assert_eq!(
        run_built_executable(&state).expect("run executable"),
        ExitCode::SUCCESS
    );
}
