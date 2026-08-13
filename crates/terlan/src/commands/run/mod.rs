use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use serde::Deserialize;

use crate::validation::target_profile::{
    explicit_target_profile_override_error, infer_target_profile_from_typed_evidence,
    TargetInferenceInput, TargetProfile,
};
use crate::{CliCommand, CliState};

const BUILD_PACKAGE_METADATA_FILE: &str = "terlan-package-build.json";

/// Runtime target selected by `terlc run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunTarget {
    TerlanVm,
}

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
///   while preserving the helper contract needed by generated NativeBoundary VM
///   stubs.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RunNativeMetadata {
    rust: Option<RunRustNativeMetadata>,
    #[serde(default)]
    rust_dependencies: Vec<RunRustNativeDependencyMetadata>,
    #[serde(default)]
    artifact_environment: Vec<RunArtifactEnvironmentMetadata>,
}

/// Prebuilt artifact runtime environment binding consumed by `terlc run`.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct RunArtifactEnvironmentMetadata {
    /// Runtime environment variable name.
    name: String,
    /// Absolute executable path in the verified package cache.
    path: String,
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
    #[serde(default)]
    target_dir: Option<String>,
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
    let cmd = match expand_script_run_command(cmd) {
        Ok(cmd) => cmd,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    if crate::commands::wasm_runtime::is_wasm_artifact_run(&cmd.args) {
        return crate::commands::wasm_runtime::run(&cmd.args);
    }

    let (cmd, program_arguments) = split_program_arguments(cmd);

    let run_target = match validate_run_args(&cmd.args) {
        Ok(target) => target,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    if let Err(message) = validate_run_target_evidence(&cmd.args, state.target_profile) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }

    let source_path = run_source_path(&cmd.args);
    let source_is_directory = source_path.is_dir();
    let build_cmd = build_command_for_run(cmd, run_target);
    let build_status = crate::commands::build::run(build_cmd, state.clone());
    if build_status != ExitCode::SUCCESS {
        return build_status;
    }

    let result = match run_target {
        RunTarget::TerlanVm if source_is_directory => {
            run_built_executable(&state, &program_arguments)
        }
        RunTarget::TerlanVm => run_built_native_image(&state, &source_path, &program_arguments),
    };

    match result {
        Ok(exit_code) => exit_code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Separates compiler/run arguments from arguments owned by the Terlan program.
///
/// The first `--` is the stable boundary. Values after it are never parsed by
/// `terlc` or forwarded to `terlc build`; they are passed unchanged as UTF-8
/// process arguments to the VM application context.
fn split_program_arguments(mut cmd: CliCommand) -> (CliCommand, Vec<String>) {
    let Some(boundary) = cmd.args.iter().position(|argument| argument == "--") else {
        return (cmd, Vec::new());
    };
    let program_arguments = cmd.args.split_off(boundary + 1);
    cmd.args.pop();
    (cmd, program_arguments)
}

/// Expands `terlc run script <name>` into a direct script source path.
///
/// Inputs:
/// - `cmd`: parsed `run` command.
///
/// Output:
/// - Original command for normal run shapes.
/// - Rewritten command whose first argument is the resolved script path when
///   the user selected a named project script.
///
/// Transformation:
/// - Resolves the script name from the current project before target
///   inference and build delegation so `run` remains a single VM execution
///   path after command-local sugar is removed.
fn expand_script_run_command(cmd: CliCommand) -> Result<CliCommand, String> {
    expand_script_run_command_in_project(cmd, Path::new("."))
}

/// Expands `terlc run script <name>` against an explicit project root.
fn expand_script_run_command_in_project(
    mut cmd: CliCommand,
    project_root: &Path,
) -> Result<CliCommand, String> {
    if cmd.args.first().map(String::as_str) != Some("script") {
        return Ok(cmd);
    }
    let name = cmd
        .args
        .get(1)
        .ok_or_else(|| "terlc run script requires a script name".to_string())?;
    if name.starts_with("--") {
        return Err("terlc run script requires a script name before options".to_string());
    }

    let script = crate::commands::scripts::resolve_project_script(project_root, name)?;
    let mut rewritten = vec![script.to_string_lossy().into_owned()];
    rewritten.extend(cmd.args.into_iter().skip(2));
    cmd.args = rewritten;
    Ok(cmd)
}

/// Validates command-local arguments accepted by `terlc run`.
///
/// Inputs:
/// - `args`: raw arguments after the `run` verb.
///
/// Output:
/// - Selected run target when the run command can forward arguments to `build`.
/// - `Err(message)` when an unsupported target is selected.
///
/// Transformation:
/// - Scans only the `--target` option so all other argument validation remains
///   owned by the build command.
fn validate_run_args(args: &[String]) -> Result<RunTarget, String> {
    let mut target = RunTarget::TerlanVm;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --target".to_string())?;
                target = match value.as_str() {
                    "erlang" => {
                        return Err(
                            "run target `erlang` was removed from the public CLI; use `terlan-vm`"
                                .to_string(),
                        );
                    }
                    "terlan-vm" => RunTarget::TerlanVm,
                    _ => {
                        return Err(format!(
                            "terlc run currently supports --target terlan-vm, got `{value}`"
                        ));
                    }
                };
                i += 2;
            }
            option if option.starts_with("--") => {
                return Err(format!("unknown run option: {option}"));
            }
            _ => i += 1,
        }
    }
    Ok(target)
}

/// Validates source-level target evidence before `run` delegates to `build`.
///
/// Inputs:
/// - `args`: command-local run arguments used to discover the source path.
/// - `requested`: global target profile selected by CLI flags.
///
/// Output:
/// - `Ok(())` when the source can be executed by `terlc run`.
/// - `Err(message)` when typed evidence requires a non-VM target or an
///   explicit target profile conflicts with the source.
///
/// Transformation:
/// - Parses Terlan modules into structured target evidence and applies the
///   same profile-inference policy as build/check before `run` appends its
///   synthetic `--target terlan-vm` build argument.
fn validate_run_target_evidence(args: &[String], requested: TargetProfile) -> Result<(), String> {
    let source_path = run_source_path(args);
    let inference = infer_run_target_profile(&source_path)?;

    if requested != TargetProfile::Vm {
        if let Some(message) = explicit_target_profile_override_error(&inference, requested) {
            return Err(format!("terlc run target inference error: {message}"));
        }
        return Err(format!(
            "terlc run target inference error: `terlc run` executes VM programs, but explicit target `{}` was requested",
            requested.as_str()
        ));
    }

    if inference.profile != TargetProfile::Vm {
        return Err(format!(
            "terlc run target inference error: `terlc run` executes VM programs, but source evidence requires `{}`",
            inference.profile.as_str()
        ));
    }

    Ok(())
}

/// Returns the source path from command-local run arguments.
///
/// Inputs:
/// - `args`: raw command-local run arguments.
///
/// Output:
/// - First positional source path, or `.` when no path was provided.
///
/// Transformation:
/// - Skips the `--target <value>` option understood by `run`; all other option
///   validation remains owned by the delegated build command.
fn run_source_path(args: &[String]) -> PathBuf {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--target" {
            i += 2;
        } else if args[i].starts_with("--") {
            i += 1;
        } else {
            return PathBuf::from(&args[i]);
        }
    }
    PathBuf::from(".")
}

/// Infers the target profile required by a run source path.
///
/// Inputs:
/// - `path`: source file or directory passed to `terlc run`.
///
/// Output:
/// - Inferred target profile with source-facing reasons.
///
/// Transformation:
/// - Parses Terlan source into syntax outputs, collects module and asset-import
///   evidence, and delegates selection to the shared target inference helper.
fn infer_run_target_profile(
    path: &Path,
) -> Result<crate::validation::target_profile::TargetInference, String> {
    let sources = run_target_inference_sources(path)?;
    let mut syntax_outputs = Vec::new();

    for source in sources {
        let text = fs::read_to_string(&source).map_err(|err| {
            format!(
                "terlc run target inference failed to read {}: {err}",
                source.display()
            )
        })?;
        let syntax =
            crate::formal_pipeline::parse_source_as_syntax_output(&source.to_string_lossy(), &text)
                .map_err(|err| {
                    format!(
                        "terlc run target inference failed to parse {}: {err:?}",
                        source.display()
                    )
                })?;
        syntax_outputs.push(syntax);
    }

    let input = TargetInferenceInput::from_syntax_modules(syntax_outputs.iter());
    infer_target_profile_from_typed_evidence(&input)
        .map_err(|conflict| format!("terlc run target inference error: {}", conflict.message))
}

/// Lists source files that participate in run-target inference.
///
/// Inputs:
/// - `path`: source file or directory passed to `terlc run`.
///
/// Output:
/// - Sorted Terlan source files used for inference.
///
/// Transformation:
/// - Uses direct file input when provided and the shared recursive source scan
///   for directory/project input. Build remains responsible for final source
///   root validation and artifact planning.
fn run_target_inference_sources(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err(format!("source path does not exist: {}", path.display()));
    }

    let mut files = crate::formal_pipeline::terlan_sources_in_dir(path)?;
    files.sort();
    Ok(files)
}

/// Builds the command forwarded from `terlc run` to `terlc build`.
///
/// Inputs:
/// - `cmd`: original run command arguments.
/// - `run_target`: target selected by run-argument validation.
///
/// Output:
/// - Build command arguments with an explicit target when the run default was
///   selected implicitly.
///
/// Transformation:
/// - Keeps `terlc run` VM-first by appending `--target terlan-vm` before
///   build delegation when the user did not provide a target. Explicit
///   user-provided targets are preserved unchanged.
fn build_command_for_run(mut cmd: CliCommand, run_target: RunTarget) -> CliCommand {
    if run_target == RunTarget::TerlanVm && !run_args_contain_target(&cmd.args) {
        cmd.args.push("--target".to_string());
        cmd.args.push("terlan-vm".to_string());
    }
    cmd
}

/// Returns whether run command arguments already contain `--target`.
///
/// Inputs:
/// - `args`: command-local run arguments.
///
/// Output:
/// - `true` when an explicit target flag is present.
///
/// Transformation:
/// - Scans the raw argument list without validating the target value, leaving
///   validation ownership with `validate_run_args`.
fn run_args_contain_target(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--target")
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
fn run_built_executable(
    state: &CliState,
    program_arguments: &[String],
) -> Result<ExitCode, String> {
    let metadata = load_run_metadata(&state.out_dir)?;
    let executable = executable_path_from_metadata(&state.out_dir, &metadata)?;
    let mut command = Command::new(&executable);
    command.args(program_arguments);
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

/// Runs the native image emitted by `terlc build --target terlan-vm`.
///
/// Inputs:
/// - `state`: parsed CLI state with the selected build output directory.
///
/// Output:
/// - `Ok(exit_code)` with the `terlan-vm run` process status.
/// - `Err(message)` when the build emitted no native image or the VM
///   runner cannot be started.
///
/// Transformation:
/// - Finds the single `.tvm` image under `_build/vm` and delegates
///   execution to the bundled `terlan-vm` binary with any verified package
///   artifact bindings recorded in build metadata.
fn run_built_native_image(
    state: &CliState,
    source_path: &Path,
    program_arguments: &[String],
) -> Result<ExitCode, String> {
    let runner = terlan_vm_runner_path()?;
    run_built_native_image_with_runner(state, &runner, source_path, program_arguments)
}

/// Runs the emitted native image with an explicit runner and package environment.
fn run_built_native_image_with_runner(
    state: &CliState,
    runner: &Path,
    source_path: &Path,
    program_arguments: &[String],
) -> Result<ExitCode, String> {
    let artifact = find_native_image_for_source(&state.out_dir, source_path)?;
    let mut command = Command::new(runner);
    command.arg("run").arg(&artifact);
    if source_path
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("terls")
    {
        command.arg("--script-eval");
    }
    command.arg("--").args(program_arguments);
    let metadata_path = state.out_dir.join(BUILD_PACKAGE_METADATA_FILE);
    if metadata_path.is_file() {
        let metadata = load_run_metadata(&state.out_dir)?;
        apply_native_helper_envs(&mut command, &metadata)?;
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to run `{}`: {err}", runner.display()))?;
    mirror_child_output(&output).map_err(|err| format!("failed to write child output: {err}"))?;
    Ok(exit_code_from_output(&output))
}

/// Finds the native image emitted for one direct source-file build.
fn find_native_image_for_source(out_dir: &Path, source_path: &Path) -> Result<PathBuf, String> {
    let source = fs::read_to_string(source_path).map_err(|err| {
        format!(
            "failed to read run source `{}`: {err}",
            source_path.display()
        )
    })?;
    let syntax = crate::formal_pipeline::parse_source_as_syntax_output(
        &source_path.to_string_lossy(),
        &source,
    )
    .map_err(|err| {
        format!(
            "failed to parse run source `{}`: {err:?}",
            source_path.display()
        )
    })?;
    let vm_dir = out_dir.join("vm");
    let native_artifact = vm_dir.join(format!("{}.tvm", syntax.module_name.replace('.', "_")));
    if native_artifact.is_file() {
        return Ok(native_artifact);
    }
    Err(format!(
        "terlc run --target terlan-vm expected native image `{}` for source `{}`",
        native_artifact.display(),
        source_path.display()
    ))
}

/// Resolves the `terlan-vm` runner beside the current executable.
fn terlan_vm_runner_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("TERLAN_VM_RUNNER") {
        return Ok(PathBuf::from(path));
    }
    let current =
        std::env::current_exe().map_err(|err| format!("failed to resolve current terlc: {err}"))?;
    let runner_name = if cfg!(windows) {
        "terlan-vm.exe"
    } else {
        "terlan-vm"
    };
    Ok(current.parent().map_or_else(
        || PathBuf::from(runner_name),
        |parent| parent.join(runner_name),
    ))
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

    for binding in &native.artifact_environment {
        push_artifact_environment(binding, &mut seen_envs, &mut bindings)?;
    }
    if let Some(rust) = &native.rust {
        push_native_helper_env(rust, &mut seen_envs, &mut bindings)?;
    }
    for dependency in &native.rust_dependencies {
        push_native_helper_env(&dependency.rust, &mut seen_envs, &mut bindings)?;
    }

    Ok(bindings)
}

/// Adds one verified prebuilt runtime binding without invoking Cargo.
fn push_artifact_environment(
    binding: &RunArtifactEnvironmentMetadata,
    seen_envs: &mut BTreeSet<String>,
    bindings: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    if std::env::var_os(&binding.name).is_some() || !seen_envs.insert(binding.name.clone()) {
        return Ok(());
    }
    let path = PathBuf::from(&binding.path);
    if !path.is_file() {
        return Err(format!(
            "cached package artifact runtime `{}` is missing: {}",
            binding.name,
            path.display()
        ));
    }
    bindings.push((binding.name.clone(), path));
    Ok(())
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
    if let Some(target_dir) = &native.target_dir {
        command.env("CARGO_TARGET_DIR", target_dir);
    }
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
    if let Some(target_dir) = &native.target_dir {
        return PathBuf::from(target_dir).join("debug").join(&native.helper);
    }
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
#[cfg(test)]
mod run_test;
