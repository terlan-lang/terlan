use super::metadata::BuildPackageMetadata;
use super::*;

mod package_outputs;
mod source_root_builds;
mod test_dependencies;

pub(in crate::commands::build) use package_outputs::write_terlan_vm_executable_package_outputs;
use package_outputs::{root_vm_service_route_sources, write_package_metadata};
pub(in crate::commands::build) use source_root_builds::*;
pub(crate) use test_dependencies::resolve_project_test_dependencies;

#[cfg(test)]
pub(super) const BUILD_DEBUG_MAP_FILE: &str = "terlan-debug-map.json";
pub(super) const BUILD_PACKAGE_METADATA_FILE: &str = "terlan-package-build.json";
pub(super) const BUILD_PACKAGE_METADATA_SCHEMA: &str = "terlan-package-build-v1";
pub(super) const TERLAN_PROJECT_MANIFEST_FILE: &str = "terlan.toml";

/// Runs the package-source command surface.
pub(crate) fn run_package_command(cmd: CliCommand, state: CliState) -> ExitCode {
    match cmd.args.first().map(String::as_str) {
        Some("protocol") => {
            crate::package_registry::run_protocol_command(&cmd.args, &state.out_dir)
        }
        Some("publish") => package_publish::run(&cmd.args, &state.out_dir),
        Some("add") => package_registry_commands::run_add(&cmd.args, &state.out_dir),
        Some("remove") => package_registry_commands::run_remove(&cmd.args, &state.out_dir),
        Some("resolve") => package_registry_resolver::run(&cmd.args, &state.out_dir),
        Some("update") => package_registry_resolver::run_update(&cmd.args, &state.out_dir),
        Some("tree") => package_registry_resolver::run_tree(&cmd.args, &state.out_dir),
        Some("audit") => {
            package_registry_audit::run(&cmd.args, &state.out_dir, state.diagnostic_format)
        }
        Some("yank") => package_registry_yank::run(&cmd.args),
        _ => package_git::run(cmd),
    }
}

pub(super) type BuildOneArtifactFn = fn(&str, &CliState) -> Result<(), BuildOneError>;

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
pub(super) struct BuildTimings {
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
    pub(in crate::commands::build) fn new(enabled: bool) -> Self {
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
    pub(in crate::commands::build) fn mark(&mut self, phase: &str) {
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
pub(super) enum BuildOneError {
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
    pub(in crate::commands::build) fn into_exit_code(self) -> ExitCode {
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
///   current helper transport is owned by `terlan-vm`, so JS and Wasm targets
///   reject that dependency instead of failing later in an emitter.
pub(super) fn validate_project_native_target(
    path: &Path,
    target: BuildTarget,
) -> Result<(), String> {
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
pub(super) fn build_target_name(target: BuildTarget) -> &'static str {
    match target {
        BuildTarget::Js(profile) => profile.as_str(),
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
pub(super) fn effective_build_target(args: &BuildArgs) -> Result<BuildTarget, String> {
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
/// - Maps maintained targets into the shared source-evidence contract.
pub(super) fn build_target_profile(target: BuildTarget) -> Option<TargetProfile> {
    match target {
        BuildTarget::Js(profile) => Some(profile),
        BuildTarget::TerlanVm => Some(TargetProfile::Vm),
        BuildTarget::WasmCore => Some(TargetProfile::WasmCore),
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
pub(super) fn infer_build_target_profile(
    path: &Path,
) -> Result<crate::validation::target_profile::TargetInference, String> {
    let sources = build_target_inference_sources(path)?;
    let mut syntax_outputs = Vec::new();

    for source in sources {
        let source_path = source.to_string_lossy();
        let text = crate::support::read_file(&source_path)
            .map_err(|err| format!("terlc build target inference failed: {err}"))?;
        let syntax = crate::formal_pipeline::parse_source_as_syntax_output(&source_path, &text)
            .map_err(|err| {
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
pub(super) fn build_target_inference_sources(path: &Path) -> Result<Vec<PathBuf>, String> {
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
pub(super) fn run_terlan_vm_build(args: &BuildArgs, state: &CliState) -> ExitCode {
    if !state.no_emit {
        let metadata_path = state.out_dir.join(BUILD_PACKAGE_METADATA_FILE);
        if let Err(error) = fs::remove_file(&metadata_path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "cannot remove stale package build metadata {}: {error}",
                    metadata_path.display()
                );
                return ExitCode::from(1);
            }
        }
    }
    let source_path = Path::new(&args.path);
    if source_path.is_dir() {
        return run_terlan_vm_directory_build(source_path, state, args.native_codegen_policy);
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
/// - `Ok(())` after the owning project's linked artifact is emitted.
/// - A typed build error when project discovery, interface preparation, source
///   layout validation, or compilation fails.
///
/// Transformation:
/// - Finds the nearest ancestor `terlan.toml`, resolves its complete package
///   source closure, and emits one linked application image. A direct file
///   cannot be compiled as an isolated AOT unit when it imports project
///   siblings. Standalone files retain the isolated build path.
pub(super) fn run_terlan_vm_file_build(
    source_path: &Path,
    state: &CliState,
    policy: crate::compiler::native_ir::NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let is_script = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("terls");
    if is_script && !script_requires_project_closure(source_path)? {
        let file_state = standalone_vm_file_state(state);
        return vm_artifact::build_one_vm_artifact(
            &source_path.to_string_lossy(),
            &file_state,
            policy,
        );
    }
    let Some(project_dir) = owning_project_dir_for_source(source_path) else {
        let file_state = standalone_vm_file_state(state);
        return vm_artifact::build_one_vm_artifact(
            &source_path.to_string_lossy(),
            &file_state,
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
    if !is_script {
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
    }

    let source_roots = roots
        .source_roots
        .iter()
        .map(|root| source_roots::SourceRootBuildUnit {
            path: root.path.clone(),
            package_path: Some(root.package_path.clone()),
        })
        .collect::<Vec<_>>();
    let (mut files, file_state) =
        prepare_source_roots_build(&source_roots, state).map_err(BuildOneError::Exit)?;
    if is_script {
        if !files.iter().any(|path| path == &canonical_source) {
            files.push(canonical_source);
        }
        let entry_module = crate::formal_pipeline::script_module_name(source_path);
        vm_artifact::build_vm_application_artifacts_with_entry(
            &files,
            &file_state,
            policy,
            &entry_module,
        )?;
    } else {
        vm_artifact::build_vm_application_artifacts(&files, &file_state, policy)?;
    }
    if !state.no_emit {
        let metadata = build_package_metadata_with_artifacts(
            &project_dir,
            &manifest,
            &roots.native_rust_dependencies,
            &roots.native_artifact_environment,
            roots.accelerator_closure.as_ref(),
        );
        write_package_metadata(&metadata, state).map_err(BuildOneError::Message)?;
    }
    Ok(())
}

/// Gives incremental standalone builds the same compiler-private cache root as
/// project and directory builds.
fn standalone_vm_file_state(state: &CliState) -> CliState {
    let mut file_state = state.clone();
    if file_state.incremental && file_state.cache_dir.is_none() {
        file_state.cache_dir = Some(file_state.out_dir.join(".terlan"));
    }
    file_state
}

/// Determines whether a direct Terlan script needs its owning package closure.
///
/// Inputs:
/// - `source_path`: direct `.terls` source selected by `terlc build` or
///   `terlc run`.
///
/// Output:
/// - `false` when the script has no imports or imports only `std.*` modules.
/// - `true` when the script imports project modules or source-backed assets.
/// - A build error when the source cannot be read or parsed.
///
/// Transformation:
/// - Parses the script through the maintained syntax pipeline and classifies
///   canonical module import identities. This lets repository validation
///   scripts remain manifest-adjacent without linking the package and its
///   native dependencies merely because the script lives below that manifest.
pub(super) fn script_requires_project_closure(source_path: &Path) -> Result<bool, BuildOneError> {
    let path = source_path.to_string_lossy();
    let source = crate::support::read_file(&path)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    let syntax =
        crate::formal_pipeline::parse_source_as_syntax_output(&path, &source).map_err(|error| {
            BuildOneError::Message(format!(
                "terlc build cannot parse script {} for import classification: {error:?}",
                source_path.display()
            ))
        })?;
    Ok(syntax.declarations.iter().any(|declaration| {
        let crate::terlan_syntax::SyntaxDeclarationPayload::Import {
            import_kind,
            module_name,
            items,
            is_selected,
            ..
        } = &declaration.payload
        else {
            return false;
        };
        if *import_kind != crate::terlan_syntax::SyntaxImportKind::Module {
            return true;
        }
        let identity =
            crate::terlan_syntax::syntax_module_import_identity(module_name, items, *is_selected);
        identity != "std" && !identity.starts_with("std.")
    }))
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
pub(super) fn owning_project_dir_for_source(source_path: &Path) -> Option<PathBuf> {
    owning_project_dir_for_source_from(source_path, &std::env::current_dir().ok()?)
}

/// Finds the nearest manifest-backed project relative to an explicit working directory.
///
/// Inputs:
/// - `source_path`: absolute or working-directory-relative source path.
/// - `current_dir`: directory used to resolve a relative source path.
///
/// Output:
/// - Nearest absolute ancestor directory containing `terlan.toml`.
/// - `None` when the source has no manifest-backed owner.
///
/// Transformation:
/// - Normalizes relative paths before ancestor traversal so the empty terminal
///   ancestor of a relative `Path` cannot be returned as a project directory.
pub(super) fn owning_project_dir_for_source_from(
    source_path: &Path,
    current_dir: &Path,
) -> Option<PathBuf> {
    let absolute_source = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        current_dir.join(source_path)
    };
    absolute_source
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
pub(super) fn run_wasm_core_build(args: &BuildArgs, state: &CliState) -> ExitCode {
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
pub(super) fn run_terlan_vm_directory_build(
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
            let route_sources = match root_vm_service_route_sources(dir, &manifest) {
                Ok(route_sources) => route_sources,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
            let vm_service = !route_sources.is_empty();
            if vm_service {
                if let Err(message) = js_browser::write_vm_service_package(
                    dir,
                    &state.out_dir,
                    &manifest.source_roots,
                    &route_sources,
                    state.incremental,
                ) {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            }
            if let Err(message) = write_terlan_vm_executable_package_outputs(
                dir,
                &manifest,
                &roots.native_rust_dependencies,
                &roots.native_artifact_environment,
                roots.accelerator_closure.as_ref(),
                vm_service,
                state,
            ) {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
            if policy == crate::compiler::native_ir::NativeCodegenPolicy::Release {
                if let Err(message) =
                    super::release_bundle::write_release_bundle(dir, &manifest, state)
                {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            }
        } else if manifest.artifact == project_manifest::ProjectArtifactKind::Library {
            let metadata = build_package_metadata_with_artifacts(
                dir,
                &manifest,
                &roots.native_rust_dependencies,
                &roots.native_artifact_environment,
                roots.accelerator_closure.as_ref(),
            );
            if let Err(message) = write_package_metadata(&metadata, state) {
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
