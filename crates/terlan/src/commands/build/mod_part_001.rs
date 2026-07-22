use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use std::fs;

use crate::commands::artifacts::fingerprint;
use crate::validation::target_profile::{
    explicit_target_profile_override_error, infer_target_profile_from_typed_evidence,
    TargetInferenceInput, TargetProfile,
};
use crate::{CliCommand, CliState};

use args::{parse_build_args, BuildArgs, BuildTarget};
use metadata::{
    build_package_metadata_with_artifacts, BuildPackageExecutable, ProjectNativeRustDependency,
};
use package_layout::{source_package_path, validate_project_source_package_root};
use project_roots::{reject_unsupported_external_dependencies, resolve_project_build_roots};
use target_gate::reject_unsupported_target_std_source;
use vm_launcher::{terlan_vm_runner_name, vm_image_has_main_entrypoint};

use metadata::ProjectSourceRoot;

/// Resolves dependency and root-package source directories for project tests.
///
/// Inputs:
/// - `project_dir`: directory containing the root package manifest.
/// - `manifest`: validated root package manifest.
///
/// Output:
/// - Dependency-first source directories using the normal build resolver.
/// - Stable package-resolution errors for missing roots, cycles, and unfetched
///   Git dependencies.
///
/// Transformation:
/// - Reuses build dependency resolution so `terlc test` observes the same
///   package graph as `terlc build` and `terlc run`.
pub(crate) fn resolve_project_test_dependencies(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
) -> Result<ResolvedProjectTestDependencies, String> {
    resolve_project_build_roots(project_dir, manifest).map(|roots| {
        let source_roots = roots
            .source_roots
            .into_iter()
            .map(|root| root.path)
            .collect();
        ResolvedProjectTestDependencies {
            source_roots,
            native_artifact_environment: roots.native_artifact_environment,
        }
    })
}

/// Dependency context shared by project builds and in-process VM tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProjectTestDependencies {
    /// Dependency-first source roots selected by normal package resolution.
    pub(crate) source_roots: Vec<PathBuf>,
    /// Verified native runtime bindings supplied by target package artifacts.
    pub(crate) native_artifact_environment: Vec<(String, PathBuf)>,
}

#[cfg(test)]
const BUILD_DEBUG_MAP_FILE: &str = "terlan-debug-map.json";
const BUILD_PACKAGE_METADATA_FILE: &str = "terlan-package-build.json";
const BUILD_PACKAGE_METADATA_SCHEMA: &str = "terlan-package-build-v1";
const TERLAN_PROJECT_MANIFEST_FILE: &str = "terlan.toml";

/// Runs the package-source command surface.
pub(crate) fn run_package_command(cmd: CliCommand) -> ExitCode {
    package_git::run(cmd)
}

type BuildOneArtifactFn = fn(&str, &CliState) -> Result<(), BuildOneError>;

/// Timing probe for optional `terlc build --timings` output.
///
/// Inputs:
/// - Wall-clock instants captured as build phases are reached.
///
/// Output:
/// - Human-readable timing lines on stderr when enabled.
///
/// Transformation:
/// - Computes phase deltas and total elapsed time without changing build
///   artifacts.
struct BuildTimings {
    enabled: bool,
    started: Instant,
    last: Instant,
}

impl BuildTimings {
    /// Creates a build timing tracker.
    ///
    /// Inputs:
    /// - `enabled`: whether timing output should be emitted.
    ///
    /// Output:
    /// - Initialized timing state anchored at the current instant.
    ///
    /// Transformation:
    /// - Captures the start and last phase clocks from the same timestamp.
    fn new(enabled: bool) -> Self {
        let now = Instant::now();
        Self {
            enabled,
            started: now,
            last: now,
        }
    }

    /// Records completion of one build phase.
    ///
    /// Inputs:
    /// - `phase`: display name for the completed phase.
    ///
    /// Output:
    /// - Optional stderr timing line.
    ///
    /// Transformation:
    /// - Converts elapsed wall-clock durations into millisecond diagnostics and
    ///   advances the phase boundary.
    fn mark(&mut self, phase: &str) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        eprintln!(
            "terlc timing: {phase}: +{}ms total={}ms",
            now.duration_since(self.last).as_millis(),
            now.duration_since(self.started).as_millis()
        );
        self.last = now;
    }
}

/// Error shape for one source build attempt.
///
/// Inputs:
/// - Created from user-facing build errors or formal pipeline exit codes.
///
/// Output:
/// - A printable error message or already-reported exit code.
///
/// Transformation:
/// - Preserves formal pipeline exit codes without inventing duplicate
///   diagnostics while still allowing build-local errors to be reported.
#[derive(Debug)]
enum BuildOneError {
    Message(String),
    Exit(ExitCode),
}

impl BuildOneError {
    /// Converts a single-source build error into the command exit code.
    ///
    /// Inputs:
    /// - `self`: build-local message or formal pipeline exit code.
    ///
    /// Output:
    /// - CLI exit code for the failed build.
    ///
    /// Transformation:
    /// - Prints build-local messages and forwards formal pipeline exit codes
    ///   whose diagnostics were already emitted by the pipeline.
    fn into_exit_code(self) -> ExitCode {
        match self {
            BuildOneError::Message(message) => {
                eprintln!("{}", message);
                ExitCode::from(1)
            }
            BuildOneError::Exit(exit_code) => exit_code,
        }
    }
}

/// Executes the `build` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing an optional source path and optional
///   command-local build flags.
/// - `state`: parsed global CLI state, including output directory, cache
///   directory, diagnostics, native policy, target profile, and no-emit mode.
///
/// Output:
/// - `ExitCode::SUCCESS` when the build succeeds.
/// - `ExitCode::from(2)` for malformed command-local arguments.
/// - `ExitCode::from(1)` for unsupported target-profile selection, source
///   reads, formal pipeline failures, output writes, artifact emission, or
///   Vm compilation failures.
///
/// Transformation:
/// - Parses build arguments and dispatches to the selected backend artifact
///   path: JavaScript, mobile planning, or the Terlan VM artifact envelope.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_build_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };
    if args.declarations && !matches!(args.target, BuildTarget::Js(_)) {
        eprintln!("terlc build --declarations requires --target js");
        return ExitCode::from(2);
    }
    if args.native_codegen_policy == crate::compiler::native_ir::NativeCodegenPolicy::Release
        && args.target_explicit
        && args.target != BuildTarget::TerlanVm
    {
        eprintln!("terlc build --release currently requires --target terlan-vm");
        return ExitCode::from(2);
    }

    let target = match effective_build_target(&args) {
        Ok(target) => target,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if args.native_codegen_policy == crate::compiler::native_ir::NativeCodegenPolicy::Release
        && target != BuildTarget::TerlanVm
    {
        eprintln!("terlc build --release currently requires --target terlan-vm");
        return ExitCode::from(2);
    }
    if let Err(message) = validate_project_native_target(Path::new(&args.path), target) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }

    match target {
        BuildTarget::Js(profile) => js::run_js_build(&args, &state, profile),
        BuildTarget::Mobile(target) => mobile::run_mobile_build(&args, &state, target),
        BuildTarget::TerlanVm => run_terlan_vm_build(&args, &state),
        BuildTarget::WasmCore => run_wasm_core_build(&args, &state),
    }
}

/// Validates package-native helper requirements against the selected target.
///
/// Inputs:
/// - `path`: build source path, which may be a manifest-backed project.
/// - `target`: effective backend target after source inference and overrides.
///
/// Output:
/// - `Ok(())` for VM builds and projects without Rust native helpers.
/// - Stable capability diagnostic for root or transitive package helpers on
///   unsupported targets.
///
/// Transformation:
/// - Resolves the local package closure before backend emission and treats a
///   `[native.rust]` helper as the `native-process-helper` capability. The
///   current helper transport is owned by `terlan-vm`, so JS, Wasm, and mobile
///   targets reject that dependency instead of failing later in an emitter.
fn validate_project_native_target(path: &Path, target: BuildTarget) -> Result<(), String> {
    if target == BuildTarget::TerlanVm || !path.is_dir() {
        return Ok(());
    }
    let manifest_path = project_manifest_path(path);
    if !manifest_path.is_file() {
        return Ok(());
    }
    let manifest = project_manifest::read_project_manifest(&manifest_path)?;
    let roots = resolve_project_build_roots(path, &manifest)?;
    let requirement = manifest
        .native_rust
        .as_ref()
        .map(|native| {
            (
                manifest.package.name.as_str(),
                native.helper.as_str(),
                "root package",
            )
        })
        .or_else(|| {
            roots.native_rust_dependencies.first().map(|dependency| {
                (
                    dependency.package.name.as_str(),
                    dependency.native.helper.as_str(),
                    dependency.origin.diagnostic_name(),
                )
            })
        });
    let Some((native_package, helper, source)) = requirement else {
        return Ok(());
    };
    Err(format!(
        "error[package_native_target_unsupported]: target `{}` cannot build package `{}` because {} `{native_package}` requires native process helper `{helper}`; capability `native-process-helper` is currently supported only by target `terlan-vm`",
        build_target_name(target),
        manifest.package.name,
        source,
    ))
}

/// Returns the stable CLI spelling for one effective build target.
fn build_target_name(target: BuildTarget) -> &'static str {
    match target {
        BuildTarget::Js(profile) => profile.as_str(),
        BuildTarget::Mobile(args::MobileBuildTarget::Android) => "mobile.android",
        BuildTarget::Mobile(args::MobileBuildTarget::Ios) => "mobile.ios",
        BuildTarget::TerlanVm => "terlan-vm",
        BuildTarget::WasmCore => "wasm.core",
    }
}

/// Resolves the build target after source-level target inference.
///
/// Inputs:
/// - Parsed build arguments including whether `--target` was explicit.
///
/// Output:
/// - Concrete backend target used by the build command.
/// - Stable target-evidence diagnostic when an explicit override conflicts.
///
/// Transformation:
/// - Defaults target-neutral code to the VM, infers JS profiles from typed
///   import/capability evidence, and treats explicit CLI targets as checked
///   overrides instead of the source of truth.
fn effective_build_target(args: &BuildArgs) -> Result<BuildTarget, String> {
    if matches!(args.target, BuildTarget::Mobile(_)) {
        return Ok(args.target);
    }

    let inference = infer_build_target_profile(Path::new(&args.path))?;
    if args.target_explicit {
        if let Some(explicit) = build_target_profile(args.target) {
            if let Some(message) = explicit_target_profile_override_error(&inference, explicit) {
                return Err(format!("terlc build target inference error: {message}"));
            }
        }
        return Ok(args.target);
    }

    Ok(match inference.profile {
        TargetProfile::WasmCore => BuildTarget::WasmCore,
        profile if profile.is_js() => BuildTarget::Js(profile),
        _ => BuildTarget::TerlanVm,
    })
}

/// Returns the target profile represented by a backend build target.
///
/// Inputs:
/// - `target`: parsed backend target.
///
/// Output:
/// - Profile for target families that participate in target inference.
///
/// Transformation:
/// - Keeps mobile planning out of the current inference contract while JS and
///   VM targets remain checked against source evidence.
fn build_target_profile(target: BuildTarget) -> Option<TargetProfile> {
    match target {
        BuildTarget::Js(profile) => Some(profile),
        BuildTarget::TerlanVm => Some(TargetProfile::Vm),
        BuildTarget::WasmCore => Some(TargetProfile::WasmCore),
        BuildTarget::Mobile(_) => None,
    }
}

/// Infers the target profile required by a build path.
///
/// Inputs:
/// - `path`: source file or directory passed to `terlc build`.
///
/// Output:
/// - Inferred target profile with source-facing reasons.
///
/// Transformation:
/// - Parses Terlan source into structured syntax output, collects module
///   imports and browser asset-import evidence, then delegates target selection
///   to the shared target-profile inference helper.
fn infer_build_target_profile(
    path: &Path,
) -> Result<crate::validation::target_profile::TargetInference, String> {
    let sources = build_target_inference_sources(path)?;
    let mut syntax_outputs = Vec::new();

    for source in sources {
        let source_path = source.to_string_lossy();
        let text = crate::support::read_file(&source_path)
            .map_err(|err| format!("terlc build target inference failed: {err}"))?;
        let syntax = crate::terlan_syntax::parse_module_as_syntax_output(&text).map_err(|err| {
            format!(
                "terlc build target inference failed to parse {}: {err:?}",
                source.display()
            )
        })?;
        syntax_outputs.push(syntax);
    }

    let input = TargetInferenceInput::from_syntax_modules(syntax_outputs.iter());
    infer_target_profile_from_typed_evidence(&input)
        .map_err(|conflict| format!("terlc build target inference error: {}", conflict.message))
}

/// Lists source files that participate in build-target inference.
///
/// Inputs:
/// - `path`: source file or directory passed to `terlc build`.
///
/// Output:
/// - Sorted Terlan source files used for inference.
///
/// Transformation:
/// - Reuses project source-root discovery when a manifest is present and falls
///   back to the same recursive source scan used by VM directory builds.
fn build_target_inference_sources(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err(format!("source path does not exist: {}", path.display()));
    }

    let source_roots = terlan_vm_source_roots_for_directory(path)?;
    let mut files = Vec::new();
    for root in source_roots {
        files.extend(crate::formal_pipeline::terlan_sources_in_dir(&root.path)?);
    }
    files.sort();
    Ok(files)
}

/// Runs the Terlan VM artifact build.
///
/// Inputs:
/// - `args`: parsed build command arguments.
/// - `state`: global CLI state used by the formal compiler pipeline.
///
/// Output:
/// - CLI exit code representing VM artifact emission success or failure.
///
/// Transformation:
/// - Emits post-OTP VM artifacts for a source file or source-root project
///   without generating Vm source or VM bytecode.
fn run_terlan_vm_build(args: &BuildArgs, state: &CliState) -> ExitCode {
    let source_path = Path::new(&args.path);
    if source_path.is_dir() {
        return run_terlan_vm_directory_build(
            source_path,
            state,
            args.native_codegen_policy,
        );
    }
    match run_terlan_vm_file_build(source_path, state, args.native_codegen_policy) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => err.into_exit_code(),
    }
}

/// Builds one VM source file with interfaces from its owning project.
///
/// Inputs:
/// - `source_path`: selected Terlan source file.
/// - `state`: global CLI state used by VM artifact emission.
///
/// Output:
/// - `Ok(())` after the selected artifact is emitted.
/// - A typed build error when project discovery, interface preparation, source
///   layout validation, or compilation fails.
///
/// Transformation:
/// - Finds the nearest ancestor `terlan.toml`, resolves its complete package
///   source closure, prepares interfaces for those roots, and emits only the
///   selected source file. Standalone files retain the isolated build path.
fn run_terlan_vm_file_build(
    source_path: &Path,
    state: &CliState,
    policy: crate::compiler::native_ir::NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let Some(project_dir) = owning_project_dir_for_source(source_path) else {
        return vm_artifact::build_one_vm_artifact(
            &source_path.to_string_lossy(),
            state,
            policy,
        );
    };
    let manifest = project_manifest::read_project_manifest(&project_manifest_path(&project_dir))
        .map_err(BuildOneError::Message)?;
    if let Some(message) = reserved_project_artifact_build_error(&manifest) {
        return Err(BuildOneError::Message(message));
    }
    let roots =
        resolve_project_build_roots(&project_dir, &manifest).map_err(BuildOneError::Message)?;
    let canonical_source = source_path.canonicalize().map_err(|err| {
        BuildOneError::Message(format!(
            "terlc build cannot canonicalize source file {}: {err}",
            source_path.display()
        ))
    })?;
    let owning_root = roots
        .source_roots
        .iter()
        .find(|root| canonical_source.starts_with(&root.path))
        .ok_or_else(|| {
            BuildOneError::Message(format!(
                "terlc build source file {} is outside the source roots declared by {}",
                source_path.display(),
                project_manifest_path(&project_dir).display()
            ))
        })?;
    validate_project_source_package_root(
        &owning_root.path,
        &canonical_source,
        &owning_root.package_path,
    )
    .map_err(BuildOneError::Message)?;

    let mut file_state = state.clone();
    if file_state.cache_dir.is_none() {
        file_state.cache_dir = Some(state.out_dir.join(".terlan"));
    }
    for root in &roots.source_roots {
        source_roots::prepare_source_root_interfaces(&root.path, &file_state)
            .map_err(BuildOneError::Message)?;
    }
    vm_artifact::build_one_vm_artifact(
        &canonical_source.to_string_lossy(),
        &file_state,
        policy,
    )
}

/// Finds the nearest manifest-backed project that contains a source file.
///
/// Inputs:
/// - `source_path`: direct source-file build path.
///
/// Output:
/// - Nearest ancestor directory containing `terlan.toml`.
/// - `None` for standalone source files.
///
/// Transformation:
/// - Walks parent directories from the source location toward the filesystem
///   root so nested projects take ownership before their ancestors.
fn owning_project_dir_for_source(source_path: &Path) -> Option<PathBuf> {
    source_path
        .parent()
        .and_then(|parent| {
            parent
                .ancestors()
                .find(|dir| project_manifest_path(dir).is_file())
        })
        .map(Path::to_path_buf)
}

/// Runs the Wasm core artifact build.
///
/// Inputs:
/// - `args`: parsed build command arguments.
/// - `state`: global CLI state used by Wasm artifact emission.
///
/// Output:
/// - CLI exit code representing Wasm core artifact emission success or
///   failure.
///
/// Transformation:
/// - Emits validated `.wasm` bytes and `.wasm.json` manifests for a source
///   file, plain directory, or manifest-backed source-root project.
fn run_wasm_core_build(args: &BuildArgs, state: &CliState) -> ExitCode {
    let source_path = Path::new(&args.path);
    if source_path.is_dir() {
        let source_roots = match terlan_vm_source_roots_for_directory(source_path) {
            Ok(source_roots) => source_roots,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        return run_wasm_core_source_roots_build(&source_roots, state);
    }
    match wasm_artifact::build_one_wasm_core_artifact(&args.path, state) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => err.into_exit_code(),
    }
}

/// Runs a Terlan VM build for a directory or manifest-backed project.
///
/// Inputs:
/// - `dir`: directory passed to `terlc build`.
/// - `state`: global CLI state used by VM artifact emission.
///
/// Output:
/// - CLI exit code representing VM artifact emission success or failure.
///
/// Transformation:
/// - Resolves `terlan.toml` source roots when present, otherwise treats `dir`
///   as one plain source root, validates manifest package layout, and emits one
///   native `.tvm` application image plus transitional compiler metadata.
fn run_terlan_vm_directory_build(
    dir: &Path,
    state: &CliState,
    policy: crate::compiler::native_ir::NativeCodegenPolicy,
) -> ExitCode {
    let manifest_path = project_manifest_path(dir);
    if manifest_path.is_file() {
        let manifest = match project_manifest::read_project_manifest(&manifest_path) {
            Ok(manifest) => manifest,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        let roots = match resolve_project_build_roots(dir, &manifest) {
            Ok(roots) => roots,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        let source_roots = roots
            .source_roots
            .iter()
            .map(|root| source_roots::SourceRootBuildUnit {
                path: root.path.clone(),
                package_path: Some(root.package_path.clone()),
            })
            .collect::<Vec<_>>();
        let status = match manifest.artifact {
            project_manifest::ProjectArtifactKind::WasmCore => {
                run_wasm_core_source_roots_build(&source_roots, state)
            }
            project_manifest::ProjectArtifactKind::TerlanVm
            | project_manifest::ProjectArtifactKind::Library => {
                run_terlan_vm_source_roots_build(&source_roots, state, policy)
            }
            _ => {
                if let Some(message) = reserved_project_artifact_build_error(&manifest) {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
                unreachable!("reserved artifact must return a diagnostic");
            }
        };
        if status != ExitCode::SUCCESS || state.no_emit {
            return status;
        }
        if manifest.artifact == project_manifest::ProjectArtifactKind::TerlanVm {
            if let Err(message) = write_terlan_vm_executable_package_outputs(
                dir,
                &manifest,
                &roots.native_rust_dependencies,
                &roots.native_artifact_environment,
                state,
            ) {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        }
        return status;
    }

    let source_roots = vec![source_roots::SourceRootBuildUnit {
        path: dir.to_path_buf(),
        package_path: None,
    }];
    run_terlan_vm_source_roots_build(&source_roots, state, policy)
}

/// Writes launcher and metadata outputs for a VM executable package.
fn write_terlan_vm_executable_package_outputs(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
    native_rust_dependencies: &[ProjectNativeRustDependency],
    native_artifact_environment: &[(String, PathBuf)],
    state: &CliState,
) -> Result<(), String> {
    let executable_name = package_executable_name(&manifest.package.name);
    let executable_relative_path = PathBuf::from("bin").join(&executable_name);
    let executable_path = state.out_dir.join(&executable_relative_path);
    let artifact_relative_path =
        PathBuf::from("vm").join(format!("{}.tvm", executable_vm_artifact_stem(manifest)));
    let artifact_path = state.out_dir.join(&artifact_relative_path);
    if !artifact_path.is_file() {
        return Err(format!(
            "terlc build executable package `{}` requires entry artifact `{}`; define `{}.Main.main/0` or set [build] artifact = \"library\"",
            manifest.package.name,
            artifact_path.display(),
            source_package_path(&manifest.package).join(".")
        ));
    }
    let entry_module = format!("{}.Main", source_package_path(&manifest.package).join("."));
    if !vm_image_has_main_entrypoint(&artifact_path, &entry_module)? {
        return Err(format!(
            "terlc build executable package `{}` requires public `main/0` in `{}`; define `{}.Main.main/0` or set [build] artifact = \"library\"",
            manifest.package.name,
            artifact_path.display(),
            source_package_path(&manifest.package).join(".")
        ));
    }
    let bundled_runner_path = state.out_dir.join("bin").join(terlan_vm_runner_name());
    copy_bundled_terlan_vm_runner(&bundled_runner_path)?;
    let bundled_worker_path = state.out_dir.join("bin").join(terlan_native_worker_name());
    copy_bundled_terlan_native_worker(&bundled_worker_path)?;
    write_vm_launcher(&executable_path, &artifact_relative_path, state.incremental)?;

    let mut metadata = build_package_metadata_with_artifacts(
        project_dir,
        manifest,
        native_rust_dependencies,
        native_artifact_environment,
    );
    metadata.executable = Some(BuildPackageExecutable {
        path: path_to_manifest_string(&executable_relative_path),
        image: path_to_manifest_string(&artifact_relative_path),
        runtime: path_to_manifest_string(&PathBuf::from("bin").join(terlan_vm_runner_name())),
        native_worker: path_to_manifest_string(
            &PathBuf::from("bin").join(terlan_native_worker_name()),
        ),
    });
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|err| format!("failed to serialize package build metadata: {err}"))?;
    write_build_file(
        &state.out_dir.join(BUILD_PACKAGE_METADATA_FILE),
        format!("{json}\n").as_bytes(),
        state.incremental,
    )?;
    Ok(())
}

/// Copies the compiler-paired VM runtime into the executable package bundle.
fn copy_bundled_terlan_vm_runner(destination: &Path) -> Result<(), String> {
    let source = terlan_vm_runner_source_path()?;
    copy_bundled_vm_runtime(&source, destination, "terlan-vm runtime")
}

/// Copies the compiler-paired native worker into the executable package bundle.
fn copy_bundled_terlan_native_worker(destination: &Path) -> Result<(), String> {
    let source = terlan_native_worker_source_path()?;
    copy_bundled_vm_runtime(&source, destination, "terlan native worker")
}

fn copy_bundled_vm_runtime(source: &Path, destination: &Path, label: &str) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create VM runtime bundle directory `{}`: {err}",
                parent.display()
            )
        })?;
    }
    fs::copy(source, destination).map_err(|err| {
        format!(
            "failed to bundle {label} from `{}` to `{}`: {err}",
            source.display(),
            destination.display()
        )
    })?;
    set_launcher_executable(destination)
}

/// Resolves the VM runtime paired with this compiler.
fn terlan_vm_runner_source_path() -> Result<PathBuf, String> {
    paired_vm_runtime_path("TERLAN_VM_RUNNER", terlan_vm_runner_name())
}

fn terlan_native_worker_name() -> &'static str {
    if cfg!(windows) {
        "terlan-native-worker.exe"
    } else {
        "terlan-native-worker"
    }
}

fn terlan_native_worker_source_path() -> Result<PathBuf, String> {
    paired_vm_runtime_path("TERLAN_NATIVE_WORKER", terlan_native_worker_name())
}

fn paired_vm_runtime_path(environment: &str, runtime_name: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(environment) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "{environment} points to missing runtime `{}`",
            path.display()
        ));
    }

    let current =
        std::env::current_exe().map_err(|err| format!("failed to resolve current terlc: {err}"))?;
    let mut candidates = Vec::new();
    if let Some(parent) = current.parent() {
        candidates.push(parent.join(runtime_name));
        if let Some(grandparent) = parent.parent() {
            candidates.push(grandparent.join(runtime_name));
        }
    }
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "terlc build could not find paired `{runtime_name}` runtime; install the complete VM runtime or set {environment}"
    ))
}

/// Writes the executable launcher for a VM artifact.
fn write_vm_launcher(
    executable_path: &Path,
    artifact_relative_path: &Path,
    incremental: bool,
) -> Result<(), String> {
    if let Some(parent) = executable_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create launcher directory `{}`: {err}",
                parent.display()
            )
        })?;
    }
    let artifact = path_to_manifest_string(artifact_relative_path);
    let contents = if cfg!(windows) {
        format!(
            "@echo off\r\nset SCRIPT_DIR=%~dp0\r\nif exist \"%SCRIPT_DIR%terlan-vm.exe\" (\r\n  \"%SCRIPT_DIR%terlan-vm.exe\" run \"%SCRIPT_DIR%..\\{}\" %*\r\n) else if not \"%TERLAN_VM_RUNNER%\"==\"\" (\r\n  \"%TERLAN_VM_RUNNER%\" run \"%SCRIPT_DIR%..\\{}\" %*\r\n) else (\r\n  terlan-vm run \"%SCRIPT_DIR%..\\{}\" %*\r\n)\r\n",
            artifact.replace('/', "\\"),
            artifact.replace('/', "\\"),
            artifact.replace('/', "\\")
        )
    } else {
        format!(
            "#!/usr/bin/env sh\nset -eu\nSCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nif [ -x \"$SCRIPT_DIR/terlan-vm\" ]; then\n  VM=\"$SCRIPT_DIR/terlan-vm\"\nelif [ -n \"${{TERLAN_VM_RUNNER:-}}\" ]; then\n  VM=$TERLAN_VM_RUNNER\nelse\n  VM=$(command -v terlan-vm)\nfi\nexec \"$VM\" run \"$SCRIPT_DIR/../{artifact}\" \"$@\"\n"
        )
    };
    write_build_file(executable_path, contents.as_bytes(), incremental)?;
    set_launcher_executable(executable_path)
}

/// Marks a launcher executable on Unix platforms.
#[cfg(unix)]
fn set_launcher_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|err| {
            format!(
                "failed to read launcher metadata `{}`: {err}",
                path.display()
            )
        })?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|err| {
        format!(
            "failed to mark launcher executable `{}`: {err}",
            path.display()
        )
    })
}

/// Marks a launcher executable on non-Unix platforms.
#[cfg(not(unix))]
fn set_launcher_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Returns the conventional executable artifact stem for a package.
fn executable_vm_artifact_stem(manifest: &project_manifest::ProjectManifest) -> String {
    let mut module = source_package_path(&manifest.package);
    module.push("Main".to_string());
    module.join("_")
}

/// Returns the emitted package executable filename.
fn package_executable_name(package_name: &str) -> String {
    if cfg!(windows) {
        format!("{package_name}.cmd")
    } else {
        package_name.to_string()
    }
}
