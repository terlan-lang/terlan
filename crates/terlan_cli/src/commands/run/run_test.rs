use super::*;

use std::time::{SystemTime, UNIX_EPOCH};

/// Creates an isolated temporary directory for run command tests.
///
/// Inputs:
/// - `name`: stable test-specific directory suffix.
///
/// Output:
/// - Empty directory under the process temporary directory.
///
/// Transformation:
/// - Adds the current process id and timestamp to avoid cross-test collisions,
///   removes stale content at the computed path, and recreates the directory.
fn make_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "terlan_run_command_{name}_{}_{}",
        std::process::id(),
        nanos
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

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

/// Verifies non-Erlang run targets are rejected before build delegation.
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
fn validate_run_args_rejects_non_erlang_target() {
    assert_eq!(
        validate_run_args(&["--target".to_string(), "js".to_string()]),
        Err("terlc run currently supports --target erlang, got `js`".to_string())
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
    let temp = make_temp_dir("metadata");
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
    let temp = make_temp_dir("missing_executable");
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
/// - Exercises helper discovery without launching Erlang so the run command
///   can set SafeNative helper env vars from build metadata.
#[cfg(unix)]
#[test]
fn discover_native_helper_envs_reads_root_and_dependency_helpers() {
    let temp = make_temp_dir("native_helper_envs");
    let root_dir = temp.join("root");
    let dep_dir = temp.join("dep");
    let root_helper = root_dir
        .join("native")
        .join("target")
        .join("debug")
        .join("root-safe-native");
    let dep_helper = dep_dir
        .join("native")
        .join("target")
        .join("debug")
        .join("dep-safe-native");
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
                        "helper":"root-safe-native",
                        "helper_env":"TERLAN_TEST_ROOT_SAFE_NATIVE_PATH_{}",
                        "package_dir":"{}"
                    }},
                    "rust_dependencies":[
                        {{
                            "package":"dep",
                            "version":"0.0.1",
                            "rust":{{
                                "path":"native",
                                "helper":"dep-safe-native",
                                "helper_env":"TERLAN_TEST_DEP_SAFE_NATIVE_PATH_{}",
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
        helper: "demo-safe-native".to_string(),
        helper_env: "DEMO_SAFE_NATIVE_PATH".to_string(),
        features: vec!["real-polars".to_string(), "csv".to_string()],
        package_dir: Some("/tmp/demo".to_string()),
    };

    assert_eq!(
        native_helper_build_args(&native),
        vec![
            "build".to_string(),
            "--manifest-path".to_string(),
            "/tmp/demo/native/Cargo.toml".to_string(),
            "--bin".to_string(),
            "demo-safe-native".to_string(),
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
///   avoiding an Erlang compiler dependency in this focused unit test.
#[cfg(unix)]
#[test]
fn run_built_executable_executes_metadata_launcher() {
    let temp = make_temp_dir("launcher");
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
