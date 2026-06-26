use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use serde::Deserialize;

use crate::{CliCommand, CliState};

const BUILD_PACKAGE_METADATA_FILE: &str = "terlan-package-build.json";

/// Minimal executable metadata consumed by `terlc run`.
///
/// Inputs:
/// - Deserialized from the `terlan-package-build.json` artifact emitted by
///   `terlc build`.
///
/// Output:
/// - Optional launcher metadata for package artifacts that can be executed.
///
/// Transformation:
/// - Ignores unrelated package metadata fields so the run command depends only
///   on the stable executable handoff contract.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RunBuildMetadata {
    executable: Option<RunExecutableMetadata>,
    native: Option<RunNativeMetadata>,
}

/// Minimal package launcher metadata consumed by `terlc run`.
///
/// Inputs:
/// - Deserialized from the `executable` section of build metadata.
///
/// Output:
/// - Relative launcher path below the selected build output directory.
///
/// Transformation:
/// - Keeps runtime and entrypoint metadata owned by the build command while the
///   run command only resolves and executes the recorded launcher path.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RunExecutableMetadata {
    path: String,
}

/// Minimal native runtime metadata consumed by `terlc run`.
///
/// Inputs:
/// - Deserialized from the optional `native` section of build metadata.
///
/// Output:
/// - Root-package and local-dependency Rust helper discovery metadata.
///
/// Transformation:
/// - Ignores native metadata for backends that `terlc run` cannot launch yet
///   while preserving the helper contract needed by generated SafeNative BEAM
///   stubs.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RunNativeMetadata {
    rust: Option<RunRustNativeMetadata>,
    #[serde(default)]
    rust_dependencies: Vec<RunRustNativeDependencyMetadata>,
}

/// Minimal Rust native helper metadata consumed by `terlc run`.
///
/// Inputs:
/// - Deserialized from `native.rust` entries in build metadata.
///
/// Output:
/// - Helper env var, helper executable name, package directory, and crate path.
///
/// Transformation:
/// - Provides only enough context to resolve the conventional Cargo debug
///   helper path when the user has already built the helper crate.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RunRustNativeMetadata {
    path: String,
    helper: String,
    helper_env: String,
    #[serde(default)]
    features: Vec<String>,
    package_dir: Option<String>,
}

/// Minimal local-dependency Rust native helper metadata consumed by `terlc run`.
///
/// Inputs:
/// - Deserialized from `native.rust_dependencies`.
///
/// Output:
/// - Nested Rust helper metadata.
///
/// Transformation:
/// - Keeps dependency package identity out of the launcher path because only
///   the nested helper contract affects process environment setup.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RunRustNativeDependencyMetadata {
    rust: RunRustNativeMetadata,
}

/// Executes the `run` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing an optional project path and build
///   target options.
/// - `state`: parsed global CLI state, including output directory and target
///   profile.
///
/// Output:
/// - `ExitCode::SUCCESS` when build and program execution succeed.
/// - `ExitCode::from(2)` for unsupported run arguments.
/// - `ExitCode::from(1)` for build failure, missing executable metadata, child
///   process errors, or child process failure.
///
/// Transformation:
/// - Validates `run` target support, delegates to `build`, reads the emitted
///   package metadata, and launches the executable recorded by the build.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    if let Err(message) = validate_run_args(&cmd.args) {
        eprintln!("{message}");
        return ExitCode::from(2);
    }

    let build_status = crate::commands::build::run(cmd.clone(), state.clone());
    if build_status != ExitCode::SUCCESS {
        return build_status;
    }

    match run_built_executable(&state) {
        Ok(exit_code) => exit_code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Validates command-local arguments accepted by `terlc run`.
///
/// Inputs:
/// - `args`: raw arguments after the `run` verb.
///
/// Output:
/// - `Ok(())` when the run command can forward the arguments to `build`.
/// - `Err(message)` when a non-Erlang target is selected.
///
/// Transformation:
/// - Scans only the `--target` option so all other argument validation remains
///   owned by the build command.
fn validate_run_args(args: &[String]) -> Result<(), String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--target" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| "missing value for --target".to_string())?;
            if value != "erlang" {
                return Err(format!(
                    "terlc run currently supports --target erlang, got `{value}`"
                ));
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(())
}

/// Runs the executable recorded in build metadata.
///
/// Inputs:
/// - `state`: parsed global CLI state with the selected build output directory.
///
/// Output:
/// - `Ok(exit_code)` with the launched process status.
/// - `Err(message)` when metadata or process execution fails.
///
/// Transformation:
/// - Resolves `terlan-package-build.json`, loads the executable path, executes
///   the launcher, mirrors child output, and converts the child status into a
///   CLI exit code.
fn run_built_executable(state: &CliState) -> Result<ExitCode, String> {
    let metadata = load_run_metadata(&state.out_dir)?;
    let executable = executable_path_from_metadata(&state.out_dir, &metadata)?;
    let mut command = Command::new(&executable);
    command.env(
        "TERLAN_SQL_RUNTIME_HELPER",
        std::env::current_exe().map_err(|err| format!("failed to resolve current terlc: {err}"))?,
    );
    apply_native_helper_envs(&mut command, &metadata)?;
    let output = command
        .output()
        .map_err(|err| format!("failed to run `{}`: {err}", executable.display()))?;
    mirror_child_output(&output).map_err(|err| format!("failed to write child output: {err}"))?;
    Ok(exit_code_from_output(&output))
}

/// Loads the package executable path from build metadata.
///
/// Inputs:
/// - `out_dir`: build output directory selected by the CLI.
///
/// Output:
/// - Absolute or current-process-relative path to the package launcher.
/// - `Err(message)` when the build metadata is absent, unreadable, invalid, or
///   lacks an executable entry.
///
/// Transformation:
/// - Deserializes the minimal executable metadata shape and joins the recorded
///   relative launcher path against the output directory.
fn load_run_metadata(out_dir: &Path) -> Result<RunBuildMetadata, String> {
    let metadata_path = out_dir.join(BUILD_PACKAGE_METADATA_FILE);
    let contents = fs::read_to_string(&metadata_path)
        .map_err(|err| format!("failed to read `{}`: {err}", metadata_path.display()))?;
    serde_json::from_str::<RunBuildMetadata>(&contents)
        .map_err(|err| format!("failed to parse `{}`: {err}", metadata_path.display()))
}

/// Resolves the package executable path from loaded build metadata.
///
/// Inputs:
/// - `out_dir`: build output directory selected by the CLI.
/// - `metadata`: parsed package build metadata.
///
/// Output:
/// - Absolute or current-process-relative path to the package launcher.
/// - `Err(message)` when metadata lacks an executable entry.
///
/// Transformation:
/// - Joins the recorded relative launcher path against the output directory.
fn executable_path_from_metadata(
    out_dir: &Path,
    metadata: &RunBuildMetadata,
) -> Result<PathBuf, String> {
    let executable = metadata.executable.as_ref().ok_or_else(|| {
        format!(
            "`{}` does not describe an executable package artifact",
            out_dir.join(BUILD_PACKAGE_METADATA_FILE).display()
        )
    })?;
    Ok(out_dir.join(&executable.path))
}

/// Applies native helper environment variables to a command.
///
/// Inputs:
/// - `command`: launcher command being prepared.
/// - `metadata`: parsed package build metadata.
///
/// Output:
/// - Mutated command environment.
///
/// Transformation:
/// - For each root or local-dependency Rust helper, sets the declared helper
///   env var when the parent process has not already set it and the
///   conventional Cargo debug helper executable exists.
fn apply_native_helper_envs(
    command: &mut Command,
    metadata: &RunBuildMetadata,
) -> Result<(), String> {
    for (env_name, helper_path) in discover_native_helper_envs(metadata)? {
        command.env(env_name, helper_path);
    }
    Ok(())
}

/// Discovers native helper environment bindings from build metadata.
///
/// Inputs:
/// - `metadata`: parsed package build metadata.
///
/// Output:
/// - Ordered helper env var/path bindings that should be applied to the child.
///
/// Transformation:
/// - Resolves helper paths under `<package_dir>/<native.path>/target/debug`,
///   skips env vars already set by the parent shell, and avoids duplicate env
///   bindings in one launcher process.
fn discover_native_helper_envs(
    metadata: &RunBuildMetadata,
) -> Result<Vec<(String, PathBuf)>, String> {
    let Some(native) = &metadata.native else {
        return Ok(Vec::new());
    };
    let mut seen_envs = BTreeSet::new();
    let mut bindings = Vec::new();

    if let Some(rust) = &native.rust {
        push_native_helper_env(rust, &mut seen_envs, &mut bindings)?;
    }
    for dependency in &native.rust_dependencies {
        push_native_helper_env(&dependency.rust, &mut seen_envs, &mut bindings)?;
    }

    Ok(bindings)
}

/// Adds one native helper binding when it is usable.
fn push_native_helper_env(
    native: &RunRustNativeMetadata,
    seen_envs: &mut BTreeSet<String>,
    bindings: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    if std::env::var_os(&native.helper_env).is_some()
        || !seen_envs.insert(native.helper_env.clone())
    {
        return Ok(());
    }
    let helper_path = ensure_native_helper_path(native)?;
    bindings.push((native.helper_env.clone(), helper_path));
    Ok(())
}

/// Ensures a Rust native helper executable exists and returns its path.
///
/// Inputs:
/// - `native`: helper metadata from package build output.
///
/// Output:
/// - Existing or newly built helper executable path.
/// - `Err(message)` when Cargo cannot build the helper or the helper path is
///   still absent after a successful Cargo invocation.
///
/// Transformation:
/// - Runs `cargo build --manifest-path <crate>/Cargo.toml --bin <helper>` when
///   the helper is missing, or when explicit features need to be applied.
fn ensure_native_helper_path(native: &RunRustNativeMetadata) -> Result<PathBuf, String> {
    if native.features.is_empty() {
        if let Some(path) = resolve_native_helper_path(native) {
            return Ok(path);
        }
    }

    build_native_helper(native)?;
    resolve_native_helper_path(native).ok_or_else(|| {
        format!(
            "native helper `{}` was not found after Cargo build at {}",
            native.helper,
            native_helper_binary_path(native).display()
        )
    })
}

/// Builds a Rust native helper executable through Cargo.
fn build_native_helper(native: &RunRustNativeMetadata) -> Result<(), String> {
    let manifest_path = native_helper_manifest_path(native);
    if !manifest_path.is_file() {
        return Err(format!(
            "native helper `{}` manifest is missing: {}",
            native.helper,
            manifest_path.display()
        ));
    }

    let mut command = Command::new("cargo");
    command.args(native_helper_build_args(native));
    let output = command.output().map_err(|err| {
        format!(
            "failed to build native helper `{}` with Cargo: {err}",
            native.helper
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "failed to build native helper `{}` with Cargo\nstdout:\n{}\nstderr:\n{}",
            native.helper, stdout, stderr
        ));
    }
    Ok(())
}

/// Returns Cargo arguments used to build one native helper.
fn native_helper_build_args(native: &RunRustNativeMetadata) -> Vec<String> {
    let mut args = vec![
        "build".to_string(),
        "--manifest-path".to_string(),
        native_helper_manifest_path(native).display().to_string(),
        "--bin".to_string(),
        native.helper.clone(),
    ];
    if !native.features.is_empty() {
        args.push("--features".to_string());
        args.push(native.features.join(","));
    }
    args
}

/// Resolves the conventional Cargo debug helper executable path.
fn resolve_native_helper_path(native: &RunRustNativeMetadata) -> Option<PathBuf> {
    let candidate = native_helper_binary_path(native);
    if candidate.is_file() {
        return Some(candidate);
    }

    #[cfg(windows)]
    {
        let exe_candidate = candidate.with_extension("exe");
        if exe_candidate.is_file() {
            return Some(exe_candidate);
        }
    }

    None
}

/// Returns the conventional Cargo debug helper executable path.
fn native_helper_binary_path(native: &RunRustNativeMetadata) -> PathBuf {
    let base = native
        .package_dir
        .as_ref()
        .map_or_else(PathBuf::new, PathBuf::from);
    base.join(&native.path)
        .join("target")
        .join("debug")
        .join(&native.helper)
}

/// Returns the Cargo manifest path for a native helper crate.
fn native_helper_manifest_path(native: &RunRustNativeMetadata) -> PathBuf {
    let base = native
        .package_dir
        .as_ref()
        .map_or_else(PathBuf::new, PathBuf::from);
    base.join(&native.path).join("Cargo.toml")
}

/// Mirrors captured child process output to the current terminal.
///
/// Inputs:
/// - `output`: completed child process output captured by `Command::output`.
///
/// Output:
/// - `Ok(())` after stdout and stderr are written.
/// - `Err(io::Error)` when either stream cannot be written.
///
/// Transformation:
/// - Replays child stdout and stderr so `terlc run` behaves like direct
///   execution while still allowing the CLI to return the child exit code.
fn mirror_child_output(output: &Output) -> io::Result<()> {
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    Ok(())
}

/// Converts a child process result into a CLI exit code.
///
/// Inputs:
/// - `output`: completed child process output with an exit status.
///
/// Output:
/// - Success when the child succeeded.
/// - The child's numeric status when available, clamped to the one-byte CLI
///   exit-code range.
/// - Generic failure when the process ended without a numeric status.
///
/// Transformation:
/// - Preserves process success/failure while adapting platform status metadata
///   into `std::process::ExitCode`.
fn exit_code_from_output(output: &Output) -> ExitCode {
    if output.status.success() {
        ExitCode::SUCCESS
    } else {
        output
            .status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or_else(|| ExitCode::from(1), ExitCode::from)
    }
}

#[cfg(test)]
#[path = "run_test.rs"]
mod run_test;
