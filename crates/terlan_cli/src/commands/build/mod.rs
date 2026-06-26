use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::time::Instant;

use terlan_erlang::{
    emit_html_runtime_to_erlang, emit_native_vector_runtime_to_erlang, emit_sql_runtime_to_erlang,
    try_emit_core_module_to_erlang_with_syntax_bridge, try_emit_syntax_struct_headers_to_hrl,
};
use terlan_hir::syntax_module_output_to_interface;
use terlan_typeck::expand_syntax_raw_macros;

use crate::commands::artifacts::{
    collect_syntax_file_import_bytes, collect_syntax_markdown_inputs,
    collect_syntax_template_inputs, fingerprint,
};
use crate::commands::source_layout::expected_module_name_for_source_path;
use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::validation::native_policy::{source_uses_native, NativePolicy};
use crate::validation::target_profile::{TargetFamily, TargetProfile, TargetProfileCheckOptions};
use crate::{CliCommand, CliState};

mod js;
mod js_assets;
mod js_browser;
mod js_model;
mod js_source_classification;
mod metadata;
mod package_artifact;
mod package_layout;
mod project_roots;
mod target_gate;
mod wasm_model;

use package_artifact::{
    validate_build_entrypoint, write_build_executable_launcher, write_build_package_metadata,
};
use package_layout::validate_project_source_package_root;
use project_roots::{reject_unsupported_external_dependencies, resolve_project_build_roots};
use target_gate::{
    reject_erlang_native_package_source, reject_unsupported_target_std_source,
    target_profile_supports_erlang_backend,
};

use metadata::{
    build_package_metadata, BuildDebugMap, BuildDebugModuleEntry, BuildDebugProject,
    BuildEntrypointFunction, BuildModuleArtifact, BuildPackageMetadata, ProjectSourceRoot,
};

pub(crate) mod project_manifest;

/// Build target accepted by `terlc build`.
///
/// Inputs:
/// - Parsed from command-local `--target` arguments.
///
/// Output:
/// - Backend target selected for artifact generation.
///
/// Transformation:
/// - Narrows free-form CLI strings to the release-supported backend set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildTarget {
    Erlang,
    Js(TargetProfile),
}

const BUILD_DEBUG_MAP_FILE: &str = "terlan-debug-map.json";
const BUILD_DEBUG_MAP_SCHEMA: &str = "terlan-build-debug-map-v1";
const BUILD_PACKAGE_METADATA_FILE: &str = "terlan-package-build.json";
const BUILD_PACKAGE_METADATA_SCHEMA: &str = "terlan-package-build-v1";
const TERLAN_PROJECT_MANIFEST_FILE: &str = "terlan.toml";

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

/// Parsed command-local arguments for `terlc build`.
///
/// Inputs:
/// - Produced from the raw command-local argument vector.
///
/// Output:
/// - One source path, one backend target, and declaration-output intent.
///
/// Transformation:
/// - Separates source selection from target selection before the build runner
///   touches the filesystem or compiler pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildArgs {
    path: String,
    target: BuildTarget,
    declarations: bool,
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
///   reads, formal pipeline failures, output writes, or Erlang compilation
///   failures.
///
/// Transformation:
/// - Parses build arguments, validates the backend profile, runs the formal
///   compiler path, emits Erlang source, and compiles it to BEAM artifacts.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_build_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    match args.target {
        BuildTarget::Erlang => {
            if args.declarations {
                eprintln!("terlc build --declarations requires --target js");
                return ExitCode::from(2);
            }
            if !target_profile_supports_erlang_backend(state.target_profile) {
                eprintln!(
                    "terlc build --target erlang requires an Erlang-compatible --target-profile, got `{}`",
                    state.target_profile.as_str()
                );
                return ExitCode::from(1);
            }
            run_erlang_build(&args, &state)
        }
        BuildTarget::Js(profile) => js::run_js_build(&args, &state, profile),
    }
}

/// Parses command-local arguments for `terlc build`.
///
/// Inputs:
/// - `args`: raw command-local arguments after global CLI parsing.
///
/// Output:
/// - `Ok(BuildArgs)` with a source path and a supported target.
/// - `Err(message)` for extra paths, unknown options, missing option values,
///   or unsupported backend targets.
///
/// Transformation:
/// - Accepts zero or one positional path and optional backend `--target`,
///   defaulting the source path to the current directory and the target to
///   Erlang when they are not specified.
fn parse_build_args(args: &[String]) -> Result<BuildArgs, String> {
    let mut path = None;
    let mut target = BuildTarget::Erlang;
    let mut declarations = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| "missing value for --target".to_string())?;
                target = parse_build_target(value)?;
                i += 2;
            }
            "--declarations" => {
                declarations = true;
                i += 1;
            }
            option if option.starts_with("--") => {
                return Err(format!("unknown build option: {option}"));
            }
            candidate => {
                if path.is_some() {
                    return Err("terlc build accepts at most one source path".to_string());
                }
                path = Some(candidate.to_string());
                i += 1;
            }
        }
    }

    let path = path.unwrap_or_else(|| ".".to_string());
    Ok(BuildArgs {
        path,
        target,
        declarations,
    })
}

/// Parses a backend target string.
///
/// Inputs:
/// - `value`: command-local target name.
///
/// Output:
/// - `Ok(BuildTarget)` for release-supported targets.
/// - `Err(message)` for unsupported targets.
///
/// Transformation:
/// - Converts the CLI spelling into the internal target enum.
fn parse_build_target(value: &str) -> Result<BuildTarget, String> {
    match value {
        "erlang" => Ok(BuildTarget::Erlang),
        js_target => crate::commands::emit_js::target_contract::parse_js_build_target_profile(
            js_target,
        )
        .map(BuildTarget::Js)
        .ok_or_else(|| {
            if let Some(family) = TargetFamily::reserved_target(js_target) {
                format!(
                    "build target `{js_target}` is reserved for the {} target family but is not implemented yet; supported targets: erlang, js, js.shared, js.browser, js.worker",
                    family.as_str()
                )
            } else {
                format!("unsupported build target `{js_target}`; supported targets: erlang, js, js.shared, js.browser, js.worker")
            }
        }),
    }
}

/// Runs the single-file Erlang build.
///
/// Inputs:
/// - `args`: parsed build command arguments.
/// - `state`: global CLI state used for output paths, diagnostics, cache, native
///   policy, and incremental writes.
///
/// Output:
/// - CLI exit code representing build success or failure.
///
/// Transformation:
/// - Dispatches directory paths to the directory build path, or reads one source
///   file, runs formal validation, emits Erlang source under `src`, and compiles
///   it with `erlc` into `ebin`.
fn run_erlang_build(args: &BuildArgs, state: &CliState) -> ExitCode {
    let source_path = Path::new(&args.path);
    if source_path.is_dir() {
        return run_erlang_directory_build(source_path, state);
    }

    build_one_erlang_source(&args.path, state)
}

/// Runs the directory Erlang build.
///
/// Inputs:
/// - `dir`: project directory to scan for `.terl` source files.
/// - `state`: global CLI state used for output paths, cache selection,
///   diagnostics, native policy, target profile, and incremental writes.
///
/// Output:
/// - CLI exit code representing directory build success or failure.
///
/// Transformation:
/// - Detects and parses `terlan.toml` project manifests before source
///   discovery. Manifest-bearing directories dispatch to the project build
///   path; plain directories dispatch to the source-root build path.
fn run_erlang_directory_build(dir: &Path, state: &CliState) -> ExitCode {
    let manifest_path = project_manifest_path(dir);
    if manifest_path.exists() {
        let manifest = match project_manifest::read_project_manifest(&manifest_path) {
            Ok(manifest) => manifest,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        return run_erlang_project_manifest_build(dir, &manifest, state);
    }

    run_erlang_source_root_build(dir, state)
}

/// Runs an Erlang build for a parsed project manifest.
///
/// Inputs:
/// - `project_dir`: directory containing `terlan.toml`.
/// - `manifest`: parsed project metadata.
/// - `state`: global CLI state used for output paths, cache selection,
///   diagnostics, native policy, target profile, and incremental writes.
///
/// Output:
/// - CLI exit code representing project build success or failure.
///
/// Transformation:
/// - Resolves manifest source roots plus local path dependency source roots
///   before backend emission, then delegates the ordered closure to the shared
///   source-root build path.
fn run_erlang_project_manifest_build(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
    state: &CliState,
) -> ExitCode {
    if let Some(message) = reserved_project_artifact_build_error(manifest) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }

    let roots = match resolve_project_build_roots(project_dir, manifest) {
        Ok(roots) => roots,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };

    run_erlang_project_source_roots_build(
        &roots.source_roots,
        state,
        Some(BuildDebugProject {
            package: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
            namespace: manifest.package.namespace.clone(),
            source_roots: manifest.source_roots.clone(),
            artifact: manifest.artifact.as_str().to_string(),
        }),
        Some(build_package_metadata(
            project_dir,
            manifest,
            &roots.native_rust_dependencies,
        )),
    )
}

/// Returns a stable diagnostic for project artifact families not owned by the
/// Erlang build path.
///
/// Inputs:
/// - Parsed project manifest.
///
/// Output:
/// - `Some(String)` when the manifest selected a reserved Wasm/WASI artifact.
/// - `None` for current Erlang-compatible artifact modes.
///
/// Transformation:
/// - Keeps manifest reservation parsing independent from build execution until
///   the Wasm/WASI dispatch gates are implemented.
fn reserved_project_artifact_build_error(
    manifest: &project_manifest::ProjectManifest,
) -> Option<String> {
    match manifest.artifact {
        project_manifest::ProjectArtifactKind::BeamThin
        | project_manifest::ProjectArtifactKind::Library => None,
        project_manifest::ProjectArtifactKind::WasmCore
        | project_manifest::ProjectArtifactKind::WasmBrowser
        | project_manifest::ProjectArtifactKind::WasmComponent => Some(format!(
            "terlc build artifact `{}` is reserved for the Wasm target family but is not implemented yet",
            manifest.artifact.as_str()
        )),
        project_manifest::ProjectArtifactKind::WasiCli
        | project_manifest::ProjectArtifactKind::WasiHttp
        | project_manifest::ProjectArtifactKind::WasiWorker => Some(format!(
            "terlc build artifact `{}` is reserved for the WASI target family but is not implemented yet",
            manifest.artifact.as_str()
        )),
    }
}

/// Runs the recursive source-root Erlang build.
///
/// Inputs:
/// - `dir`: source root to scan for `.terl` source files.
/// - `state`: global CLI state used for output paths, cache selection,
///   diagnostics, native policy, target profile, and incremental writes.
///
/// Output:
/// - CLI exit code representing source-root build success or failure.
///
/// Transformation:
/// - Discovers source files, validates the source root through the existing
///   check command with a build-local interface cache, then emits and compiles
///   each source file through the formal single-file pipeline using that cache.
fn run_erlang_source_root_build(dir: &Path, state: &CliState) -> ExitCode {
    run_erlang_plain_source_roots_build(&[dir.to_path_buf()], state, None, None)
}

/// Runs the recursive Erlang build for one or more source roots.
///
/// Inputs:
/// - `source_roots`: source roots to scan for `.terl` source files.
/// - `state`: global CLI state used for output paths, cache selection,
///   diagnostics, native policy, target profile, and incremental writes.
/// - `project`: optional project metadata to include in the build debug map.
/// - `package_metadata`: optional project package metadata to write beside
///   the debug map after successful manifest-backed builds.
///
/// Output:
/// - CLI exit code representing source-root build success or failure.
///
/// Transformation:
/// - Discovers sources in every root, validates each root through the existing
///   check command with a shared build-local interface cache, emits all modules,
///   compiles them to BEAM, writes one combined debug map, and writes optional
///   package/build metadata for manifest-backed package builds.
fn run_erlang_plain_source_roots_build(
    source_roots: &[PathBuf],
    state: &CliState,
    project: Option<BuildDebugProject>,
    package_metadata: Option<BuildPackageMetadata>,
) -> ExitCode {
    let source_roots = source_roots
        .iter()
        .map(|path| SourceRootBuildUnit {
            path: path.clone(),
            package_path: None,
        })
        .collect::<Vec<_>>();
    run_erlang_source_roots_build(&source_roots, state, project, package_metadata)
}

/// Runs the recursive Erlang build for manifest-backed project roots.
///
/// Inputs:
/// - `source_roots`: source roots carrying manifest package-root identity.
/// - `state`: global CLI state used for output paths, cache selection,
///   diagnostics, native policy, target profile, and incremental writes.
/// - `project`: optional project metadata to include in the build debug map.
/// - `package_metadata`: optional project package metadata to write beside
///   the debug map after successful manifest-backed builds.
///
/// Output:
/// - CLI exit code representing source-root build success or failure.
///
/// Transformation:
/// - Converts project source roots into build units that enforce the package
///   root segment before delegating to the shared source-root build path.
fn run_erlang_project_source_roots_build(
    source_roots: &[ProjectSourceRoot],
    state: &CliState,
    project: Option<BuildDebugProject>,
    package_metadata: Option<BuildPackageMetadata>,
) -> ExitCode {
    let source_roots = source_roots
        .iter()
        .map(|root| SourceRootBuildUnit {
            path: root.path.clone(),
            package_path: Some(root.package_path.clone()),
        })
        .collect::<Vec<_>>();
    run_erlang_source_roots_build(&source_roots, state, project, package_metadata)
}

/// Source root consumed by the shared build path.
///
/// Inputs:
/// - Produced from plain directory roots or manifest-backed source roots.
///
/// Output:
/// - Build-local root path plus optional package-root enforcement.
///
/// Transformation:
/// - Lets plain directory builds keep source-root-relative module layout while
///   manifest builds require the first source path segment to match the package
///   root declared by `terlan.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceRootBuildUnit {
    path: PathBuf,
    package_path: Option<Vec<String>>,
}

/// Runs the recursive Erlang build for one or more source roots.
///
/// Inputs:
/// - `source_roots`: source roots to scan for `.terl` source files.
/// - `state`: global CLI state used for output paths, cache selection,
///   diagnostics, native policy, target profile, and incremental writes.
/// - `project`: optional project metadata to include in the build debug map.
/// - `package_metadata`: optional project package metadata to write beside
///   the debug map after successful manifest-backed builds.
///
/// Output:
/// - CLI exit code representing source-root build success or failure.
///
/// Transformation:
/// - Discovers sources in every root, validates package-root path layout when
///   a manifest provided package identity, validates each root through the
///   existing check command with a shared build-local interface cache, emits
///   all modules, compiles them to BEAM, writes one combined debug map, and
///   writes optional package/build metadata for manifest-backed package builds.
fn run_erlang_source_roots_build(
    source_roots: &[SourceRootBuildUnit],
    state: &CliState,
    project: Option<BuildDebugProject>,
    package_metadata: Option<BuildPackageMetadata>,
) -> ExitCode {
    let mut timings = BuildTimings::new(state.timings);
    let mut files = Vec::new();
    for root in source_roots {
        let root_files = match crate::formal_pipeline::terlan_sources_in_dir(&root.path) {
            Ok(root_files) => root_files,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        if root_files.is_empty() {
            report_empty_source_root(&root.path);
            return ExitCode::from(1);
        }
        if let Some(package_path) = root.package_path.as_deref() {
            for file in &root_files {
                if let Err(message) =
                    validate_project_source_package_root(&root.path, file, package_path)
                {
                    eprintln!("{}", message);
                    return ExitCode::from(1);
                }
            }
        }
        files.extend(root_files);
    }
    timings.mark("erlang.scan");

    let mut directory_state = state.clone();
    if directory_state.cache_dir.is_none() {
        directory_state.cache_dir = Some(state.out_dir.join(".terlan"));
    }

    if directory_state.incremental {
        for root in source_roots {
            if let Err(message) = prepare_source_root_interfaces(&root.path, &directory_state) {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        }
        timings.mark("erlang.interface-prepass");
    } else {
        let check_status = run_full_source_root_checks(source_roots, &directory_state);
        if check_status != ExitCode::SUCCESS {
            return check_status;
        }
        timings.mark("erlang.full-check");
    }

    let module_artifacts = match build_erlang_source_artifacts(&files, &directory_state) {
        Ok(artifacts) => artifacts,
        Err(err) => {
            if !directory_state.incremental {
                return err.into_exit_code();
            }
            let check_status = run_full_source_root_checks(source_roots, &directory_state);
            if check_status != ExitCode::SUCCESS {
                return check_status;
            }
            match build_erlang_source_artifacts(&files, &directory_state) {
                Ok(artifacts) => artifacts,
                Err(err) => return err.into_exit_code(),
            }
        }
    };
    timings.mark("erlang.compile");

    if state.no_emit {
        return ExitCode::SUCCESS;
    }

    let entrypoint = if let Some(metadata) = package_metadata.as_ref() {
        if metadata.executable.is_some() {
            match validate_build_entrypoint(&module_artifacts, metadata) {
                Ok(entrypoint) => Some(entrypoint),
                Err(message) => {
                    eprintln!("{}", message);
                    return ExitCode::from(1);
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let debug_entries = module_artifacts
        .into_iter()
        .map(|artifact| artifact.debug_entry)
        .collect::<Vec<_>>();

    if let Err(message) = write_build_debug_map(
        &directory_state.out_dir,
        project,
        debug_entries,
        directory_state.incremental,
    ) {
        eprintln!("{}", message);
        return ExitCode::from(1);
    }

    if let Some(metadata) = package_metadata {
        if let Some(executable) = metadata.executable.as_ref() {
            let Some(entrypoint) = entrypoint else {
                eprintln!(
                    "internal build error: executable package metadata was present without a validated entrypoint"
                );
                return ExitCode::from(1);
            };
            if let Err(message) = write_build_executable_launcher(
                &directory_state.out_dir,
                executable,
                &entrypoint,
                directory_state.incremental,
            ) {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        }
        if let Err(message) = write_build_package_metadata(
            &directory_state.out_dir,
            metadata,
            directory_state.incremental,
        ) {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Runs formal checks for every source root before Erlang artifact emission.
///
/// Inputs:
/// - `source_roots`: resolved source roots in the current build.
/// - `state`: CLI state reused by the check command.
///
/// Output:
/// - Success when all roots pass, or the first failing check exit code.
///
/// Transformation:
/// - Delegates each root to `terlc check` so build output uses the same
///   diagnostics as explicit validation.
fn run_full_source_root_checks(source_roots: &[SourceRootBuildUnit], state: &CliState) -> ExitCode {
    for root in source_roots {
        let check_status = crate::commands::check::run_check_dir(
            &root.path.to_string_lossy(),
            state.clone(),
            None,
        );
        if check_status != ExitCode::SUCCESS {
            return check_status;
        }
    }
    ExitCode::SUCCESS
}

/// Emits Erlang build artifacts for a list of Terlan source files.
///
/// Inputs:
/// - `files`: source files selected for the Erlang build.
/// - `state`: build-local CLI state including output and cache directories.
///
/// Output:
/// - Module artifacts ready for metadata and launcher generation.
///
/// Transformation:
/// - Compiles each file through the formal single-source Erlang path and drops
///   files that intentionally produce no runtime module.
fn build_erlang_source_artifacts(
    files: &[PathBuf],
    state: &CliState,
) -> Result<Vec<BuildModuleArtifact>, BuildOneError> {
    let mut module_artifacts = Vec::new();
    for file in files {
        match build_one_erlang_source_artifact(&file.to_string_lossy(), state) {
            Ok(Some(artifact)) => module_artifacts.push(artifact),
            Ok(None) => {}
            Err(err) => return Err(err),
        }
    }
    Ok(module_artifacts)
}

/// Writes project-local interfaces needed by per-file build compilation.
///
/// This is intentionally narrower than `terlc check`: it parses and validates
/// module layout, then writes `.typi` files so the following per-module build
/// pass can resolve imports while doing the actual typecheck only once.
pub(super) fn prepare_source_root_interfaces(root: &Path, state: &CliState) -> Result<(), String> {
    let cache_dir = state
        .cache_dir
        .as_deref()
        .ok_or_else(|| "internal build error: interface cache directory missing".to_string())?;
    fs::create_dir_all(cache_dir).map_err(|err| {
        format!(
            "cannot create cache directory {}: {err}",
            cache_dir.display()
        )
    })?;
    let files = crate::formal_pipeline::terlan_sources_in_dir(root)?;
    for file in files {
        let path_text = file.to_string_lossy().to_string();
        let source = crate::support::read_file(&path_text)?;
        let syntax_output = crate::formal_pipeline::parse_source_as_syntax_output(
            &path_text, &source,
        )
        .map_err(|err| {
            format!(
                "cannot parse source {} during build interface prepass: {err:?}",
                path_text
            )
        })?;
        let (syntax_output, macro_diagnostics) = expand_syntax_raw_macros(syntax_output);
        if let Some(diagnostic) = macro_diagnostics.first() {
            return Err(format!(
                "{}: macro expansion failed during build interface prepass: {}",
                path_text, diagnostic.message
            ));
        }
        validate_build_directory_module_layout(root, &file, &syntax_output.module_name)?;
        let interface = syntax_module_output_to_interface(&syntax_output);
        let target = cache_dir.join(format!("{}.typi", syntax_output.module_name));
        write_build_file(
            &target,
            interface.to_terlan_interface_text().as_bytes(),
            state.incremental,
        )?;
    }
    Ok(())
}

/// Validates that a module declaration matches its source-root-relative path.
///
/// Inputs:
/// - `root`: source root that owns the file.
/// - `file`: source file being prepared for build.
/// - `module_name`: module declared by the source file.
///
/// Output:
/// - Success for a matching declaration or a stable layout error.
///
/// Transformation:
/// - Derives the expected module from the path and compares it to source text.
fn validate_build_directory_module_layout(
    root: &Path,
    file: &Path,
    module_name: &str,
) -> Result<(), String> {
    let expected = expected_module_name_for_source_path(root, file)?;
    if expected == module_name {
        return Ok(());
    }
    Err(format!(
        "module declaration `{module_name}` does not match source path `{}`; expected `module {expected}.`",
        file.display()
    ))
}

/// Reports an empty build source root with nested-project guidance.
///
/// Inputs:
/// - `root`: source root that produced no `.terl` files.
///
/// Output:
/// - User-facing diagnostic text on stderr.
///
/// Transformation:
/// - Looks for nested `terlan.toml` project roots below the empty source root
///   and, when present, adds a concrete command hint so parent scratch
///   directories do not look like broken module-layout roots.
fn report_empty_source_root(root: &Path) {
    let nested_projects = nested_project_roots(root).unwrap_or_default();
    if nested_projects.is_empty() {
        eprintln!("terlc build found no .terl files in {}", root.display());
        return;
    }

    let projects = nested_projects
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "terlc build found no .terl files in {}. Found nested Terlan project(s): {projects}. Run `terlc build <project>` or `cd <project> && terlc build`.",
        root.display()
    );
}

/// Finds nested Terlan project roots under a directory.
///
/// Inputs:
/// - `root`: directory to scan for child project manifests.
///
/// Output:
/// - Sorted nested directories containing `terlan.toml`.
/// - `Err(message)` when the filesystem cannot be read.
///
/// Transformation:
/// - Recursively walks deterministic directory entries, records child
///   directories containing the canonical manifest, and does not descend into
///   a recorded project root.
fn nested_project_roots(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut projects = Vec::new();
    collect_nested_project_roots(root, &mut projects)?;
    projects.sort();
    Ok(projects)
}

/// Recursively collects nested Terlan project roots.
///
/// Inputs:
/// - `dir`: directory currently being scanned.
/// - `projects`: mutable list of discovered nested project roots.
///
/// Output:
/// - `Ok(())` when scan completes.
/// - `Err(message)` when an entry or file type cannot be read.
///
/// Transformation:
/// - Reads one directory level, sorts child entries, records manifest-bearing
///   child directories, and only descends into non-project directories.
fn collect_nested_project_roots(dir: &Path, projects: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read dir {}: {}", dir.display(), err))?;
    let mut children = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read dir entry: {err}"))?;
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "failed to read file type for {}: {err}",
                entry.path().display()
            )
        })?;
        children.push((entry.path(), file_type));
    }
    children.sort_by(|left, right| left.0.cmp(&right.0));

    for (path, file_type) in children {
        if !file_type.is_dir() {
            continue;
        }
        if path.join(TERLAN_PROJECT_MANIFEST_FILE).is_file() {
            projects.push(path);
            continue;
        }
        collect_nested_project_roots(&path, projects)?;
    }
    Ok(())
}

/// Computes the canonical project manifest path for a build directory.
///
/// Inputs:
/// - `dir`: directory passed to `terlc build`.
///
/// Output:
/// - Path to the package/project manifest candidate inside `dir`.
///
/// Transformation:
/// - Appends the canonical manifest filename without reading or parsing it so
///   directory builds can reject project manifests before silently treating
///   them as plain source roots.
fn project_manifest_path(dir: &Path) -> PathBuf {
    dir.join(TERLAN_PROJECT_MANIFEST_FILE)
}

/// Builds one Erlang source artifact from one Terlan source file.
///
/// Inputs:
/// - `path`: Terlan source path to read and compile.
/// - `state`: global CLI state used for diagnostics, cache, native policy,
///   target profile, output directory, and incremental writes.
///
/// Output:
/// - CLI exit code representing single-module build success or failure.
///
/// Transformation:
/// - Reads a source file, runs the formal compiler path with asset imports
///   enabled for build artifacts, emits Erlang source, and compiles the result
///   to BEAM.
fn build_one_erlang_source(path: &str, state: &CliState) -> ExitCode {
    match build_one_erlang_source_artifact(path, state) {
        Ok(Some(artifact)) => {
            match write_build_debug_map(
                &state.out_dir,
                None,
                vec![artifact.debug_entry],
                state.incremental,
            ) {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("{}", message);
                    ExitCode::from(1)
                }
            }
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(err) => err.into_exit_code(),
    }
}

/// Builds one Erlang source artifact and returns debug metadata.
///
/// Inputs:
/// - `path`: Terlan source path to read and compile.
/// - `state`: global CLI state used for diagnostics, cache, native policy,
///   target profile, output directory, and incremental writes.
///
/// Output:
/// - `Ok(Some(entry))` when artifacts are emitted and compiled.
/// - `Ok(None)` when `--no-emit` suppresses artifact output.
/// - `Err(BuildOneError)` for source reads, formal pipeline failures, backend
///   emission, output writes, or Erlang compilation failures.
///
/// Transformation:
/// - Runs one source file through the formal compiler path, writes backend
///   artifacts when enabled, and captures source-to-artifact debug identity.
fn build_one_erlang_source_artifact(
    path: &str,
    state: &CliState,
) -> Result<Option<BuildModuleArtifact>, BuildOneError> {
    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => return Err(BuildOneError::Message(message)),
    };
    if let Err(message) = reject_erlang_native_package_source(path, &source) {
        return Err(BuildOneError::Message(message));
    }
    let target_profile_options = build_target_profile_options(state, true);
    if let Err(message) = reject_unsupported_target_std_source(
        path,
        &source,
        state.target_profile,
        target_profile_options,
    ) {
        return Err(BuildOneError::Message(message));
    }

    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
            target_profile_options,
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return Err(BuildOneError::Exit(exit_code)),
        };

    if state.no_emit {
        return Ok(None);
    }

    write_and_compile_erlang_build(path, &compiled, state)
        .map(Some)
        .map_err(BuildOneError::Message)
}

/// Builds command-owned target-profile options for Erlang package emission.
///
/// Inputs:
/// - `state`: global CLI state carrying native policy.
/// - `allow_asset_imports`: whether this command owns asset import resolution.
///
/// Output:
/// - Target-profile validation options aligned with build command packaging.
///
/// Transformation:
/// - Admits Rust-backed std modules only when the selected native policy is not
///   pure, keeping `--native-policy pure` as an explicit no-bridge build mode.
fn build_target_profile_options(
    state: &CliState,
    allow_asset_imports: bool,
) -> TargetProfileCheckOptions {
    TargetProfileCheckOptions {
        allow_asset_imports,
        allow_rust_backed_std_modules: state.native_policy != NativePolicy::Pure,
    }
}

/// Writes Erlang build sources and compiles them to BEAM.
///
/// Inputs:
/// - `source_path`: Terlan source path used to resolve relative imports.
/// - `compiled`: formal pipeline artifacts for one module.
/// - `state`: global CLI state containing output and incremental-write options.
///
/// Output:
/// - `Ok(BuildDebugModuleEntry)` after `.erl`, optional `.hrl`, optional
///   runtime files, and BEAM artifacts are produced.
/// - `Err(message)` for dependency collection, emitter, directory, write, or
///   `erlc` failures.
///
/// Transformation:
/// - Converts checked compiler artifacts into the build directory layout:
///   `src/*.erl`, `src/*.hrl`, and `ebin/*.beam`, then returns debug metadata
///   that traces those artifacts back to source and CoreIR identity.
fn write_and_compile_erlang_build(
    source_path: &str,
    compiled: &CheckedSyntaxModuleArtifacts,
    state: &CliState,
) -> Result<BuildModuleArtifact, String> {
    let source_dir = state.out_dir.join("src");
    let ebin_dir = state.out_dir.join("ebin");
    fs::create_dir_all(&source_dir)
        .map_err(|err| format!("cannot create build source directory: {err}"))?;
    fs::create_dir_all(&ebin_dir)
        .map_err(|err| format!("cannot create build ebin directory: {err}"))?;

    let output_stem = crate::support::erlang_output_stem(&compiled.syntax_output.module_name);
    let erl_path = source_dir.join(format!("{output_stem}.erl"));
    let code = emit_compiled_erlang_source(source_path, compiled)?;
    write_build_file(&erl_path, code.as_bytes(), state.incremental)?;

    let hrl = try_emit_syntax_struct_headers_to_hrl(&compiled.syntax_output)?;
    if !hrl.is_empty() {
        let hrl_path = source_dir.join(format!("{output_stem}.hrl"));
        write_build_file(&hrl_path, hrl.as_bytes(), state.incremental)?;
    }

    if crate::commands::static_site::syntax_module_uses_html(&compiled.syntax_output) {
        let runtime_path = source_dir.join("typer_html.erl");
        write_build_file(
            &runtime_path,
            emit_html_runtime_to_erlang().as_bytes(),
            state.incremental,
        )?;
        compile_erlang_source(&source_dir, &ebin_dir, &runtime_path, state.incremental)?;
    }

    if compiled.core.uses_sql_runtime_boundary() {
        let runtime_path = source_dir.join("terlan_sql_runtime.erl");
        write_build_file(
            &runtime_path,
            emit_sql_runtime_to_erlang().as_bytes(),
            state.incremental,
        )?;
        compile_erlang_source(&source_dir, &ebin_dir, &runtime_path, state.incremental)?;
    }

    if compiled_module_uses_native_vector(compiled) {
        let runtime_path = source_dir.join("std_native_collections_vector_safe_native.erl");
        write_build_file(
            &runtime_path,
            emit_native_vector_runtime_to_erlang().as_bytes(),
            state.incremental,
        )?;
        compile_erlang_source(&source_dir, &ebin_dir, &runtime_path, state.incremental)?;
    }

    emit_and_compile_safe_native_stubs(source_path, &source_dir, &ebin_dir, state)?;

    compile_erlang_source(&source_dir, &ebin_dir, &erl_path, state.incremental)?;

    Ok(BuildModuleArtifact {
        debug_entry: BuildDebugModuleEntry {
            module: compiled.syntax_output.module_name.clone(),
            source_path: source_path.to_string(),
            core_ir_hash: fingerprint(compiled.core.contract_text().as_bytes()),
            erl_path: path_to_manifest_string(&erl_path),
            beam_path: path_to_manifest_string(&ebin_dir.join(format!("{output_stem}.beam"))),
        },
        functions: compiled
            .core
            .functions
            .iter()
            .map(|function| BuildEntrypointFunction {
                name: function.name.clone(),
                arity: function.arity,
                public: function.public,
                return_type: function.return_type.clone(),
            })
            .collect(),
    })
}

/// Emits and compiles SafeNative stubs for compiler-native package modules.
///
/// Inputs:
/// - `source_path`: Terlan source path being built.
/// - `source_dir`: build source directory that will receive generated Erlang.
/// - `ebin_dir`: build BEAM output directory.
/// - `state`: CLI state carrying native policy and incremental behavior.
///
/// Output:
/// - `Ok(())` when no native stubs are needed or all generated stubs compile.
/// - `Err(message)` for source reads, metadata emission, or Erlang compile
///   failures.
///
/// Transformation:
/// - Reuses the existing SafeNative metadata emitter during normal build/test
///   paths so `@compiler.native` declarations call a concrete Erlang not-loaded
///   stub instead of lowering the literal `native` expression.
fn emit_and_compile_safe_native_stubs(
    source_path: &str,
    source_dir: &Path,
    ebin_dir: &Path,
    state: &CliState,
) -> Result<(), String> {
    let source = crate::support::read_file(source_path)?;
    if !source_uses_native(&source) {
        return Ok(());
    }

    crate::commands::emit_native_metadata::emit_native_artifacts(
        &source,
        source_dir,
        state.native_policy,
        state.incremental,
    )?;

    for entry in fs::read_dir(source_dir)
        .map_err(|err| format!("cannot read build source directory: {err}"))?
    {
        let path = entry
            .map_err(|err| format!("cannot read build source entry: {err}"))?
            .path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.ends_with("_safe_native.erl") {
            compile_erlang_source(source_dir, ebin_dir, &path, state.incremental)?;
        }
    }

    Ok(())
}

/// Returns whether compiled artifacts need the native vector bridge runtime.
///
/// Inputs:
/// - `compiled`: checked compiler artifacts for one source module.
///
/// Output:
/// - `true` when the module imports `std.native.collections.Vector`.
///
/// Transformation:
/// - Reads formal CoreIR import metadata instead of scanning source text so
///   runtime companion emission follows the same resolved import identity used
///   by target-profile validation and backend lowering.
fn compiled_module_uses_native_vector(compiled: &CheckedSyntaxModuleArtifacts) -> bool {
    compiled
        .core
        .imports
        .iter()
        .any(|import| import.module == "std.native.collections.Vector")
}

/// Writes the build debug map into the build output directory.
///
/// Inputs:
/// - `out_dir`: build output root.
/// - `project`: optional project metadata for manifest-backed builds.
/// - `modules`: successfully built module debug entries.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after the debug map exists or there are no emitted modules.
/// - `Err(message)` when serialization or writing fails.
///
/// Transformation:
/// - Serializes source-to-artifact metadata as stable JSON at
///   `terlan-debug-map.json`.
fn write_build_debug_map(
    out_dir: &Path,
    project: Option<BuildDebugProject>,
    modules: Vec<BuildDebugModuleEntry>,
    incremental: bool,
) -> Result<(), String> {
    if modules.is_empty() {
        return Ok(());
    }

    let map = BuildDebugMap {
        schema: BUILD_DEBUG_MAP_SCHEMA,
        target: "erlang",
        project,
        modules,
    };
    let json = serde_json::to_string_pretty(&map)
        .map_err(|err| format!("failed to serialize build debug map: {err}"))?;
    write_build_file(
        &out_dir.join(BUILD_DEBUG_MAP_FILE),
        format!("{json}\n").as_bytes(),
        incremental,
    )
}

/// Converts a filesystem path into the debug-map string representation.
///
/// Inputs:
/// - `path`: generated artifact path.
///
/// Output:
/// - Lossy UTF-8 string suitable for JSON manifests.
///
/// Transformation:
/// - Uses display-compatible path conversion so debug maps remain readable
///   across Unix and non-Unix environments.
fn path_to_manifest_string(path: &Path) -> String {
    PathBuf::from(path).to_string_lossy().into_owned()
}

/// Emits Erlang source for checked Terlan artifacts.
///
/// Inputs:
/// - `source_path`: Terlan source path used to collect relative file imports.
/// - `compiled`: formal pipeline artifacts carrying CoreIR, syntax output, and
///   dependency interfaces.
///
/// Output:
/// - Erlang source text on success.
/// - `Err(message)` when source imports, template inputs, markdown inputs, or
///   backend emission fail.
///
/// Transformation:
/// - Collects command-local asset inputs and delegates CoreIR-to-Erlang lowering
///   to the backend emitter.
fn emit_compiled_erlang_source(
    source_path: &str,
    compiled: &CheckedSyntaxModuleArtifacts,
) -> Result<String, String> {
    let source_path = Path::new(source_path);
    let file_imports = collect_syntax_file_import_bytes(&compiled.syntax_output, source_path)?;
    let templates = collect_syntax_template_inputs(&compiled.syntax_output, source_path)?;
    let markdown_imports = collect_syntax_markdown_inputs(&compiled.syntax_output, source_path)?;
    let interfaces = compiled
        .interfaces
        .iter()
        .map(|(name, interface)| (name.clone(), interface.clone()))
        .collect::<BTreeMap<_, _>>();

    try_emit_core_module_to_erlang_with_syntax_bridge(
        &compiled.core,
        &compiled.syntax_output,
        &interfaces,
        &file_imports,
        &templates,
        &markdown_imports,
    )
}

/// Writes one build output file.
///
/// Inputs:
/// - `path`: output path to write.
/// - `bytes`: file contents.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after the file exists with the requested contents.
/// - `Err(message)` when the write fails.
///
/// Transformation:
/// - Delegates to the shared incremental-write helper and wraps errors with the
///   build output path.
fn write_build_file(path: &Path, bytes: &[u8], incremental: bool) -> Result<(), String> {
    crate::support::write_if_changed_or_forced(path, bytes, incremental)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

/// Compiles one Erlang source file into the build `ebin` directory.
///
/// Inputs:
/// - `source_dir`: build source directory used as the Erlang include path.
/// - `ebin_dir`: destination directory for generated `.beam` files.
/// - `erl_path`: Erlang source file to compile.
/// - `incremental`: whether an already-current `.beam` may be reused.
///
/// Output:
/// - `Ok(())` when `erlc` exits successfully.
/// - `Err(message)` for process spawn failures or non-zero compiler exits.
///
/// Transformation:
/// - In incremental mode, skips `erlc` when the destination `.beam` is newer
///   than the `.erl` and generated headers. Otherwise runs
///   `erlc -I <source_dir> -o <ebin_dir> <erl_path>` with crash dumps
///   redirected outside the build source tree.
fn compile_erlang_source(
    source_dir: &Path,
    ebin_dir: &Path,
    erl_path: &Path,
    incremental: bool,
) -> Result<(), String> {
    if incremental && erlang_source_compile_is_current(source_dir, ebin_dir, erl_path)? {
        return Ok(());
    }

    let crash_dump = ebin_dir.join("erl_crash.dump");
    let mut command = Command::new("erlc");
    command
        .arg("-I")
        .arg(source_dir)
        .arg("-o")
        .arg(ebin_dir)
        .arg(erl_path);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erlc", Some(&crash_dump))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        Err(format!(
            "erlc failed for {} with status {}",
            erl_path.display(),
            output.status
        ))
    } else {
        Err(format!(
            "erlc failed for {}: {}",
            erl_path.display(),
            stderr
        ))
    }
}

/// Returns whether an Erlang source already has a current BEAM artifact.
///
/// Inputs:
/// - `source_dir`: generated Erlang source directory containing `.erl` and
///   optional `.hrl` files.
/// - `ebin_dir`: generated BEAM output directory.
/// - `erl_path`: generated Erlang source being considered for compilation.
///
/// Output:
/// - `Ok(true)` when the expected `.beam` exists and is newer than the source
///   and every generated header.
/// - `Ok(false)` when the source should be compiled.
/// - `Err(message)` when filesystem metadata cannot be read.
///
/// Transformation:
/// - Maps `foo.erl` to `foo.beam`, compares filesystem modification times,
///   and treats any newer generated header as invalidating the BEAM. Header
///   invalidation is intentionally conservative because the current bridge may
///   include generated records from any source module.
fn erlang_source_compile_is_current(
    source_dir: &Path,
    ebin_dir: &Path,
    erl_path: &Path,
) -> Result<bool, String> {
    let Some(stem) = erl_path.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(false);
    };
    let beam_path = ebin_dir.join(format!("{stem}.beam"));
    if !beam_path.exists() {
        return Ok(false);
    }

    let beam_modified = file_modified_at(&beam_path)?;
    if file_modified_at(erl_path)? > beam_modified {
        return Ok(false);
    }

    let entries = fs::read_dir(source_dir)
        .map_err(|err| format!("failed to read build source directory: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read build source entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("hrl")
            && file_modified_at(&path)? > beam_modified
        {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Reads the modification time for a generated artifact.
///
/// Inputs:
/// - `path`: generated source, header, or BEAM artifact path.
///
/// Output:
/// - Filesystem modification timestamp.
/// - `Err(message)` when metadata or modification time cannot be read.
///
/// Transformation:
/// - Wraps `std::fs::metadata(...).modified()` with build-oriented error
///   context so incremental compiler-cache decisions fail visibly.
fn file_modified_at(path: &Path) -> Result<std::time::SystemTime, String> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(|err| {
            format!(
                "failed to read modification time for {}: {err}",
                path.display()
            )
        })
}

/// Runs a process while preventing local Erlang crash dumps in source output.
///
/// Inputs:
/// - `command`: process builder to execute.
/// - `label`: human-readable tool name used in spawn failures.
/// - `erl_crash_dump`: optional path assigned to `ERL_CRASH_DUMP`.
///
/// Output:
/// - `Ok(Output)` when the process starts and exits.
/// - `Err(message)` when the process cannot be spawned.
///
/// Transformation:
/// - Adds the Erlang crash-dump environment override and delegates to
///   `Command::output`.
fn run_command_with_no_erl_crash_dump(
    command: &mut Command,
    label: &str,
    erl_crash_dump: Option<&Path>,
) -> Result<Output, String> {
    if let Some(path) = erl_crash_dump {
        command.env("ERL_CRASH_DUMP", path);
    }
    command
        .output()
        .map_err(|err| format!("failed to run {label}: {err}"))
}

#[cfg(test)]
mod build_test;
