use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use serde::Serialize;
use terlan_erlang::{
    emit_html_runtime_to_erlang, try_emit_core_module_to_erlang_with_syntax_bridge,
    try_emit_syntax_struct_headers_to_hrl,
};

use crate::commands::artifacts::{
    collect_syntax_file_import_bytes, collect_syntax_markdown_inputs,
    collect_syntax_template_inputs, fingerprint,
};
use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};
use crate::{CliCommand, CliState};

mod project_manifest;

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
}

const BUILD_DEBUG_MAP_FILE: &str = "terlan-debug-map.json";
const BUILD_DEBUG_MAP_SCHEMA: &str = "terlan-build-debug-map-v1";
const BUILD_PACKAGE_METADATA_FILE: &str = "terlan-package-build.json";
const BUILD_PACKAGE_METADATA_SCHEMA: &str = "terlan-package-build-v1";
const TERLAN_PROJECT_MANIFEST_FILE: &str = "terlan.toml";

/// Serializable source-to-artifact debug map for one build invocation.
///
/// Inputs:
/// - Produced from successfully compiled build module entries.
///
/// Output:
/// - JSON-ready metadata written to the build output directory.
///
/// Transformation:
/// - Groups backend artifact paths under a stable schema so debuggers,
///   release tools, and future backend runners can trace generated artifacts
///   back to Terlan source and CoreIR identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildDebugMap {
    schema: &'static str,
    target: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<BuildDebugProject>,
    modules: Vec<BuildDebugModuleEntry>,
}

/// Serializable project metadata for a manifest-backed build invocation.
///
/// Inputs:
/// - Produced from parsed `terlan.toml` metadata.
///
/// Output:
/// - Optional project entry inside `terlan-debug-map.json`.
///
/// Transformation:
/// - Records package identity, manifest source roots, and selected artifact
///   kind so project-level build artifacts can be traced back to package
///   metadata as well as source files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildDebugProject {
    package: String,
    version: String,
    source_roots: Vec<String>,
    artifact: String,
}

/// Serializable package/build metadata for a manifest-backed build.
///
/// Inputs:
/// - Produced from a parsed root `terlan.toml`.
///
/// Output:
/// - JSON-ready package metadata written beside backend artifacts.
///
/// Transformation:
/// - Separates package identity, artifact selection, source roots, and
///   dependency metadata from the source-to-backend debug map so downstream
///   tools can reason about package shape without consuming debug traces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildPackageMetadata {
    schema: &'static str,
    target: &'static str,
    package: BuildPackageIdentity,
    artifact: String,
    executable: BuildPackageExecutable,
    source_roots: Vec<String>,
    dependencies: Vec<BuildPackageDependency>,
    adapters: Vec<BuildPackageAdapter>,
}

/// Serializable package identity inside build metadata.
///
/// Inputs:
/// - Produced from the manifest `[package]` table.
///
/// Output:
/// - Stable package name/version payload.
///
/// Transformation:
/// - Copies the validated package identity into the build artifact metadata
///   schema without adding target-specific package-manager semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildPackageIdentity {
    name: String,
    version: String,
}

/// Serializable single executable artifact metadata.
///
/// Inputs:
/// - Produced from the selected package artifact mode and package identity.
///
/// Output:
/// - Executable artifact entry inside `terlan-package-build.json`.
///
/// Transformation:
/// - Records the user-facing executable path and runtime expectation while
///   keeping backend `.erl` and `.beam` files classified as intermediate
///   compiler artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildPackageExecutable {
    mode: String,
    path: String,
    runtime: String,
    entrypoint: BuildPackageEntrypoint,
}

/// Serializable entrypoint metadata inside executable build metadata.
///
/// Inputs:
/// - Produced from the manifest package name and selected artifact mode.
///
/// Output:
/// - Stable package entrypoint module/function/arity payload.
///
/// Transformation:
/// - Converts the package-root convention into metadata consumed by the
///   launcher writer and future release/debug tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildPackageEntrypoint {
    module: String,
    function: String,
    arity: usize,
}

/// Serializable dependency metadata inside build metadata.
///
/// Inputs:
/// - Produced from parsed manifest dependency entries.
///
/// Output:
/// - One normalized dependency entry in `terlan-package-build.json`.
///
/// Transformation:
/// - Represents every accepted dependency source kind with stable string
///   fields while omitting fields that do not apply to that source kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildPackageDependency {
    alias: String,
    scope: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

/// Serializable target package-adapter metadata inside build metadata.
///
/// Inputs:
/// - Produced from target package-adapter reservations in `terlan.toml`.
///
/// Output:
/// - One normalized adapter entry in `terlan-package-build.json`.
///
/// Transformation:
/// - Records target-owned adapter intent without generating adapter files or
///   making target package tools part of the generic build path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildPackageAdapter {
    target: String,
    adapter: String,
}

/// Resolved project package build roots.
///
/// Inputs:
/// - Produced from a root project manifest plus recursively parsed local
///   `path` dependencies.
///
/// Output:
/// - Ordered source roots for validation/emission.
///
/// Transformation:
/// - Keeps dependency source roots before the root package source roots so
///   imports from the root package can resolve through the shared build cache.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectBuildRoots {
    source_roots: Vec<ProjectSourceRoot>,
}

/// Resolved source root with package identity.
///
/// Inputs:
/// - Produced from a project manifest or local path dependency manifest.
///
/// Output:
/// - Filesystem source root plus the source package root required under that
///   root for module-layout validation.
///
/// Transformation:
/// - Carries manifest package identity into the shared source-root build path
///   so package-root imports are validated before CoreIR/backend emission.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSourceRoot {
    path: PathBuf,
    package_root: String,
}

/// Serializable debug metadata for one compiled module.
///
/// Inputs:
/// - Produced after Erlang source and BEAM artifact generation succeeds.
///
/// Output:
/// - One module entry inside `terlan-debug-map.json`.
///
/// Transformation:
/// - Records the source path, CoreIR hash, generated Erlang source path, and
///   generated BEAM path for source-to-artifact debugging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BuildDebugModuleEntry {
    module: String,
    source_path: String,
    core_ir_hash: u64,
    erl_path: String,
    beam_path: String,
}

/// Built module artifact plus entrypoint-relevant CoreIR summary.
///
/// Inputs:
/// - Produced after one source file has compiled to Erlang and BEAM artifacts.
///
/// Output:
/// - Debug-map entry plus public/private function summaries used by package
///   executable validation.
///
/// Transformation:
/// - Keeps executable entrypoint validation on CoreIR facts without adding
///   function signatures to the public debug-map JSON schema.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildModuleArtifact {
    debug_entry: BuildDebugModuleEntry,
    functions: Vec<BuildEntrypointFunction>,
}

/// Entrypoint-relevant function summary for one built module.
///
/// Inputs:
/// - Extracted from `CoreFunction` after the formal compiler path succeeds.
///
/// Output:
/// - Minimal name/arity/visibility/return-type payload for launcher contract
///   validation.
///
/// Transformation:
/// - Projects CoreIR function declarations into a build-local summary so the
///   executable gate does not depend on backend syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildEntrypointFunction {
    name: String,
    arity: usize,
    public: bool,
    return_type: String,
}

/// Validated executable entrypoint for a package build.
///
/// Inputs:
/// - Produced by checking manifest-derived executable metadata against built
///   module CoreIR summaries.
///
/// Output:
/// - Terlan module/function identity and backend Erlang module/function names.
///
/// Transformation:
/// - Bridges the target-neutral package entrypoint convention to the concrete
///   BEAM invocation owned by the `beam-thin` launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildEntrypoint {
    module: String,
    function: String,
    arity: usize,
    erlang_module: String,
    erlang_function: String,
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
/// - One source path and one backend target.
///
/// Transformation:
/// - Separates source selection from target selection before the build runner
///   touches the filesystem or compiler pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildArgs {
    path: String,
    target: BuildTarget,
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

    if !target_profile_supports_erlang_backend(state.target_profile) {
        eprintln!(
            "terlc build --target erlang requires an Erlang-compatible --target-profile, got `{}`",
            state.target_profile.as_str()
        );
        return ExitCode::from(1);
    }

    match args.target {
        BuildTarget::Erlang => run_erlang_build(&args, &state),
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
/// - Accepts zero or one positional path and optional `--target erlang`,
///   defaulting the source path to the current directory and the target to
///   Erlang when they are not specified.
fn parse_build_args(args: &[String]) -> Result<BuildArgs, String> {
    let mut path = None;
    let mut target = BuildTarget::Erlang;
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
    Ok(BuildArgs { path, target })
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
        other => Err(format!(
            "unsupported build target `{other}`; supported targets: erlang"
        )),
    }
}

/// Returns whether a target profile can produce Erlang artifacts.
///
/// Inputs:
/// - `profile`: globally selected target-profile gate.
///
/// Output:
/// - `true` when the profile is Erlang-compatible.
///
/// Transformation:
/// - Treats the general `erlang` profile and release-slice `*-erlang` profiles
///   as valid build gates, while rejecting backend-agnostic profiles such as
///   `core-v0`.
fn target_profile_supports_erlang_backend(profile: TargetProfile) -> bool {
    profile == TargetProfile::Erlang || profile.as_str().ends_with("-erlang")
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
/// - `dir`: project directory to scan for `.tl` source files.
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

/// Resolves project and local path dependency source roots.
///
/// Inputs:
/// - `project_dir`: root package directory.
/// - `manifest`: parsed root package manifest.
///
/// Output:
/// - Ordered project source roots, including local dependency roots first.
/// - `Err(String)` when a local dependency path is invalid, lacks
///   `terlan.toml`, has a missing source root, or participates in a cycle.
///
/// Transformation:
/// - Recursively walks only local `path` dependencies and leaves target-scoped
///   external dependency metadata for later target-adapter diagnostics.
fn resolve_project_build_roots(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
) -> Result<ProjectBuildRoots, String> {
    reject_unsupported_external_dependencies(manifest)?;
    let mut resolver = LocalDependencyResolver::default();
    let root_dir = canonical_project_dir(project_dir)?;
    resolver.resolve_package(&root_dir, manifest)?;
    Ok(ProjectBuildRoots {
        source_roots: resolver.source_roots,
    })
}

/// Rejects target-scoped external dependency metadata for the current build.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - `Ok(())` when no unsupported external dependency metadata is present.
/// - `Err(String)` with a stable diagnostic for the first unsupported external
///   dependency.
///
/// Transformation:
/// - Allows local path dependencies to continue into closure validation and
///   stops `hex`, `npm`, and `cargo` dependencies before backend emission until
///   target package-manager adapters land.
fn reject_unsupported_external_dependencies(
    manifest: &project_manifest::ProjectManifest,
) -> Result<(), String> {
    for dependency in &manifest.dependencies {
        if let Some((target, source, package, version)) = external_dependency_metadata(dependency) {
            return Err(format!(
                "terlc build package `{}` declares unsupported {} dependency `{}` from {} package `{}` version `{}`; package-manager integration is not available in A0.42.4",
                manifest.package.name,
                target,
                dependency.alias,
                source,
                package,
                version
            ));
        }
    }
    Ok(())
}

/// Extracts target-scoped external dependency metadata.
///
/// Inputs:
/// - `dependency`: parsed project dependency.
///
/// Output:
/// - Target name, source kind, package name, and version for external
///   dependencies.
/// - `None` for local path dependencies.
///
/// Transformation:
/// - Converts dependency enum variants into diagnostic strings without
///   changing dependency resolution state.
fn external_dependency_metadata(
    dependency: &project_manifest::ProjectDependency,
) -> Option<(&'static str, &'static str, &str, &str)> {
    match (&dependency.scope, &dependency.source) {
        (
            project_manifest::ProjectDependencyScope::Target(
                project_manifest::ProjectTarget::Erlang,
            ),
            project_manifest::ProjectDependencySource::Hex { package, version },
        ) => Some(("erlang", "hex", package.as_str(), version.as_str())),
        (
            project_manifest::ProjectDependencyScope::Target(project_manifest::ProjectTarget::Js),
            project_manifest::ProjectDependencySource::Npm { package, version },
        ) => Some(("js", "npm", package.as_str(), version.as_str())),
        (
            project_manifest::ProjectDependencyScope::Target(project_manifest::ProjectTarget::Rust),
            project_manifest::ProjectDependencySource::Cargo { package, version },
        ) => Some(("rust", "cargo", package.as_str(), version.as_str())),
        _ => None,
    }
}

/// Resolver state for local path dependency closure.
///
/// Inputs:
/// - Created per project build.
///
/// Output:
/// - Accumulates ordered source roots and cycle/duplicate tracking state.
///
/// Transformation:
/// - Tracks packages currently being visited separately from packages already
///   resolved, so dependency cycles can be rejected before backend emission.
#[derive(Debug, Default)]
struct LocalDependencyResolver {
    visiting: BTreeSet<PathBuf>,
    visited: BTreeSet<PathBuf>,
    source_roots: Vec<ProjectSourceRoot>,
}

impl LocalDependencyResolver {
    /// Resolves one package and its local path dependencies.
    ///
    /// Inputs:
    /// - `project_dir`: canonical package directory.
    /// - `manifest`: parsed package manifest.
    ///
    /// Output:
    /// - `Ok(())` after dependency roots and package roots are appended.
    /// - `Err(String)` for cycles, invalid dependency manifests, or missing
    ///   source roots.
    ///
    /// Transformation:
    /// - Performs depth-first dependency traversal so dependencies are emitted
    ///   before dependents.
    fn resolve_package(
        &mut self,
        project_dir: &Path,
        manifest: &project_manifest::ProjectManifest,
    ) -> Result<(), String> {
        if self.visited.contains(project_dir) {
            return Ok(());
        }
        if !self.visiting.insert(project_dir.to_path_buf()) {
            return Err(format!(
                "terlc build local path dependency cycle includes package `{}` at {}",
                manifest.package.name,
                project_dir.display()
            ));
        }

        for dependency in &manifest.dependencies {
            if let project_manifest::ProjectDependencySource::Path { path } = &dependency.source {
                let dependency_dir =
                    canonical_dependency_dir(project_dir, &dependency.alias, path)?;
                let dependency_manifest_path = project_manifest_path(&dependency_dir);
                if !dependency_manifest_path.is_file() {
                    return Err(format!(
                        "terlc build local path dependency `{}` does not contain {}: {}",
                        dependency.alias,
                        TERLAN_PROJECT_MANIFEST_FILE,
                        dependency_manifest_path.display()
                    ));
                }
                let dependency_manifest =
                    project_manifest::read_project_manifest(&dependency_manifest_path)?;
                self.resolve_package(&dependency_dir, &dependency_manifest)?;
            }
        }

        for root in &manifest.source_roots {
            let source_root = project_dir.join(root);
            if !source_root.is_dir() {
                return Err(format!(
                    "terlc build package `{}` source root does not exist: {}",
                    manifest.package.name,
                    source_root.display()
                ));
            }
            self.source_roots.push(ProjectSourceRoot {
                path: source_root,
                package_root: source_package_root(&manifest.package.name),
            });
        }

        self.visiting.remove(project_dir);
        self.visited.insert(project_dir.to_path_buf());
        Ok(())
    }
}

/// Canonicalizes a project directory for closure tracking.
///
/// Inputs:
/// - `project_dir`: package directory path.
///
/// Output:
/// - Canonical absolute package directory.
///
/// Transformation:
/// - Uses filesystem canonicalization so equivalent relative paths are treated
///   as the same package during duplicate and cycle detection.
fn canonical_project_dir(project_dir: &Path) -> Result<PathBuf, String> {
    project_dir.canonicalize().map_err(|err| {
        format!(
            "terlc build cannot canonicalize project directory {}: {err}",
            project_dir.display()
        )
    })
}

/// Canonicalizes a local path dependency directory.
///
/// Inputs:
/// - `project_dir`: canonical depending package directory.
/// - `alias`: dependency alias used in diagnostics.
/// - `path`: manifest path dependency value.
///
/// Output:
/// - Canonical dependency package directory.
///
/// Transformation:
/// - Resolves the dependency path relative to the depending package directory
///   and canonicalizes it before closure traversal.
fn canonical_dependency_dir(
    project_dir: &Path,
    alias: &str,
    path: &str,
) -> Result<PathBuf, String> {
    let dependency_dir = project_dir.join(path);
    dependency_dir.canonicalize().map_err(|err| {
        format!(
            "terlc build local path dependency `{}` cannot be resolved: {} ({err})",
            alias,
            dependency_dir.display()
        )
    })
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
            source_roots: manifest.source_roots.clone(),
            artifact: manifest.artifact.as_str().to_string(),
        }),
        Some(build_package_metadata(manifest)),
    )
}

/// Builds deterministic package metadata from a parsed project manifest.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Serializable package/build metadata for artifact consumers.
///
/// Transformation:
/// - Copies validated package fields and converts dependency enum variants to a
///   sorted, string-keyed metadata schema without resolving external packages.
fn build_package_metadata(manifest: &project_manifest::ProjectManifest) -> BuildPackageMetadata {
    let mut dependencies = manifest
        .dependencies
        .iter()
        .map(build_package_dependency_metadata)
        .collect::<Vec<_>>();
    dependencies.sort_by(|left, right| {
        (
            left.scope.as_str(),
            left.alias.as_str(),
            left.source.as_str(),
            left.path.as_deref().unwrap_or(""),
            left.package.as_deref().unwrap_or(""),
            left.version.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.scope.as_str(),
                right.alias.as_str(),
                right.source.as_str(),
                right.path.as_deref().unwrap_or(""),
                right.package.as_deref().unwrap_or(""),
                right.version.as_deref().unwrap_or(""),
            ))
    });

    BuildPackageMetadata {
        schema: BUILD_PACKAGE_METADATA_SCHEMA,
        target: "erlang",
        package: BuildPackageIdentity {
            name: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
        },
        artifact: manifest.artifact.as_str().to_string(),
        executable: build_package_executable_metadata(manifest),
        source_roots: manifest.source_roots.clone(),
        dependencies,
        adapters: build_package_adapter_metadata(manifest),
    }
}

/// Builds deterministic executable artifact metadata.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Serializable executable artifact metadata for the selected target.
///
/// Transformation:
/// - Converts the current `beam-thin` artifact mode into the launcher path and
///   external runtime expectation admitted by the A0.42.7 executable contract.
fn build_package_executable_metadata(
    manifest: &project_manifest::ProjectManifest,
) -> BuildPackageExecutable {
    match manifest.artifact {
        project_manifest::ProjectArtifactKind::BeamThin => BuildPackageExecutable {
            mode: "beam-thin".to_string(),
            path: format!("bin/{}", manifest.package.name),
            runtime: "external-erts".to_string(),
            entrypoint: BuildPackageEntrypoint {
                module: format!("{}.Main", source_package_root(&manifest.package.name)),
                function: "main".to_string(),
                arity: 0,
            },
        },
    }
}

/// Builds deterministic target package-adapter metadata.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Ordered adapter metadata entries for the package build artifact.
///
/// Transformation:
/// - Preserves supported target adapter reservations as metadata only; it does
///   not generate Rebar3 files, package-manager manifests, or release configs.
fn build_package_adapter_metadata(
    manifest: &project_manifest::ProjectManifest,
) -> Vec<BuildPackageAdapter> {
    manifest
        .erlang_package_adapter
        .map(|adapter| BuildPackageAdapter {
            target: "erlang".to_string(),
            adapter: adapter.as_str().to_string(),
        })
        .into_iter()
        .collect()
}

/// Builds one deterministic dependency metadata entry.
///
/// Inputs:
/// - `dependency`: parsed manifest dependency.
///
/// Output:
/// - Serializable dependency metadata for the package build artifact.
///
/// Transformation:
/// - Converts local and target-scoped dependency source variants into stable
///   strings while preserving the original package alias and source metadata.
fn build_package_dependency_metadata(
    dependency: &project_manifest::ProjectDependency,
) -> BuildPackageDependency {
    match &dependency.source {
        project_manifest::ProjectDependencySource::Path { path } => BuildPackageDependency {
            alias: dependency.alias.clone(),
            scope: package_dependency_scope(&dependency.scope).to_string(),
            source: "path".to_string(),
            path: Some(path.clone()),
            package: None,
            version: None,
        },
        project_manifest::ProjectDependencySource::Hex { package, version } => {
            BuildPackageDependency {
                alias: dependency.alias.clone(),
                scope: package_dependency_scope(&dependency.scope).to_string(),
                source: "hex".to_string(),
                path: None,
                package: Some(package.clone()),
                version: Some(version.clone()),
            }
        }
        project_manifest::ProjectDependencySource::Npm { package, version } => {
            BuildPackageDependency {
                alias: dependency.alias.clone(),
                scope: package_dependency_scope(&dependency.scope).to_string(),
                source: "npm".to_string(),
                path: None,
                package: Some(package.clone()),
                version: Some(version.clone()),
            }
        }
        project_manifest::ProjectDependencySource::Cargo { package, version } => {
            BuildPackageDependency {
                alias: dependency.alias.clone(),
                scope: package_dependency_scope(&dependency.scope).to_string(),
                source: "cargo".to_string(),
                path: None,
                package: Some(package.clone()),
                version: Some(version.clone()),
            }
        }
    }
}

/// Returns the package metadata spelling for a dependency scope.
///
/// Inputs:
/// - `scope`: parsed dependency scope.
///
/// Output:
/// - Stable scope string for build metadata.
///
/// Transformation:
/// - Converts local and target-specific dependency scopes to the same section
///   names used by the manifest contract.
fn package_dependency_scope(scope: &project_manifest::ProjectDependencyScope) -> &'static str {
    match scope {
        project_manifest::ProjectDependencyScope::Local => "local",
        project_manifest::ProjectDependencyScope::Target(
            project_manifest::ProjectTarget::Erlang,
        ) => "target.erlang",
        project_manifest::ProjectDependencyScope::Target(project_manifest::ProjectTarget::Js) => {
            "target.js"
        }
        project_manifest::ProjectDependencyScope::Target(project_manifest::ProjectTarget::Rust) => {
            "target.rust"
        }
    }
}

/// Runs the recursive source-root Erlang build.
///
/// Inputs:
/// - `dir`: source root to scan for `.tl` source files.
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
/// - `source_roots`: source roots to scan for `.tl` source files.
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
            package_root: None,
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
            package_root: Some(root.package_root.clone()),
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
    package_root: Option<String>,
}

/// Runs the recursive Erlang build for one or more source roots.
///
/// Inputs:
/// - `source_roots`: source roots to scan for `.tl` source files.
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
            eprintln!("terlc build found no .tl files in {}", root.path.display());
            return ExitCode::from(1);
        }
        if let Some(package_root) = root.package_root.as_deref() {
            for file in &root_files {
                if let Err(message) =
                    validate_project_source_package_root(&root.path, file, package_root)
                {
                    eprintln!("{}", message);
                    return ExitCode::from(1);
                }
            }
        }
        files.extend(root_files);
    }

    let mut directory_state = state.clone();
    if directory_state.cache_dir.is_none() {
        directory_state.cache_dir = Some(state.out_dir.join(".terlan"));
    }

    for root in source_roots {
        let check_status = crate::commands::check::run_check_dir(
            &root.path.to_string_lossy(),
            directory_state.clone(),
            None,
        );
        if check_status != ExitCode::SUCCESS {
            return check_status;
        }
    }
    if state.no_emit {
        return ExitCode::SUCCESS;
    }

    let mut module_artifacts = Vec::new();
    for file in files {
        match build_one_erlang_source_artifact(&file.to_string_lossy(), &directory_state) {
            Ok(Some(artifact)) => module_artifacts.push(artifact),
            Ok(None) => {}
            Err(err) => return err.into_exit_code(),
        }
    }

    let entrypoint = if let Some(metadata) = package_metadata.as_ref() {
        match validate_build_entrypoint(&module_artifacts, metadata) {
            Ok(entrypoint) => Some(entrypoint),
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
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
        let Some(entrypoint) = entrypoint else {
            eprintln!(
                "internal build error: manifest-backed package metadata was present without a validated entrypoint"
            );
            return ExitCode::from(1);
        };
        if let Err(message) = write_build_executable_launcher(
            &directory_state.out_dir,
            &metadata,
            &entrypoint,
            directory_state.incremental,
        ) {
            eprintln!("{}", message);
            return ExitCode::from(1);
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

/// Converts a package name into the source module root spelling.
///
/// Inputs:
/// - `package_name`: manifest `[package] name` value.
///
/// Output:
/// - Lowercase module-root spelling used in source layout validation.
///
/// Transformation:
/// - Replaces package-manager dashes with underscores because Terlan module
///   path segments use `LowerIdent`, while package names may contain `-`.
fn source_package_root(package_name: &str) -> String {
    package_name.replace('-', "_")
}

/// Validates that a manifest source file starts under the package root.
///
/// Inputs:
/// - `source_root`: manifest-declared source root.
/// - `file`: discovered Terlan source file under the source root.
/// - `package_root`: normalized package root expected as the first relative
///   source path segment.
///
/// Output:
/// - `Ok(())` when the file path starts with the package root.
/// - `Err(message)` when the file is outside the root, contains non-UTF-8
///   path segments, or has a different first segment.
///
/// Transformation:
/// - Checks source-root-relative paths before the existing `terlc check <dir>`
///   pass validates the full module declaration against that path.
fn validate_project_source_package_root(
    source_root: &Path,
    file: &Path,
    package_root: &str,
) -> Result<(), String> {
    let relative = file.strip_prefix(source_root).map_err(|_| {
        format!(
            "source file `{}` is not under project source root `{}`",
            file.display(),
            source_root.display()
        )
    })?;
    let mut components = relative.components();
    let first = components.next().ok_or_else(|| {
        format!(
            "source path `{}` has no package root segment",
            file.display()
        )
    })?;
    let first = first.as_os_str().to_str().ok_or_else(|| {
        format!(
            "source path `{}` contains a non-UTF-8 package root segment",
            file.display()
        )
    })?;
    if first == package_root {
        return Ok(());
    }
    Err(format!(
        "project source file `{}` is outside package root `{}`; expected path under `{}/{}`",
        file.display(),
        package_root,
        source_root.display(),
        package_root
    ))
}

/// Validates the manifest-backed package executable entrypoint.
///
/// Inputs:
/// - `modules`: build artifacts and CoreIR function summaries for every
///   emitted package module.
/// - `metadata`: manifest-derived package/build metadata.
///
/// Output:
/// - `Ok(BuildEntrypoint)` when `<package_root>.Main.main(): Unit` exists,
///   is public, and has arity zero.
/// - `Err(message)` when the entrypoint module, function, visibility, arity,
///   or return type violates the package executable contract.
///
/// Transformation:
/// - Checks the package-root entrypoint convention against backend-neutral
///   CoreIR summaries before any user-facing executable launcher is written.
fn validate_build_entrypoint(
    modules: &[BuildModuleArtifact],
    metadata: &BuildPackageMetadata,
) -> Result<BuildEntrypoint, String> {
    let expected = &metadata.executable.entrypoint;
    let module = modules
        .iter()
        .find(|artifact| artifact.debug_entry.module == expected.module)
        .ok_or_else(|| {
            format!(
                "terlc build package `{}` requires entrypoint `{}.{}(): Unit`; module `{}` was not built",
                metadata.package.name, expected.module, expected.function, expected.module
            )
        })?;

    let matching_arity = module
        .functions
        .iter()
        .find(|function| function.name == expected.function && function.arity == expected.arity);
    let Some(function) = matching_arity else {
        let arities = module
            .functions
            .iter()
            .filter(|function| function.name == expected.function)
            .map(|function| function.arity.to_string())
            .collect::<Vec<_>>();
        if arities.is_empty() {
            return Err(format!(
                "terlc build package `{}` requires entrypoint `{}.{}(): Unit`; function `{}` is missing from module `{}`",
                metadata.package.name,
                expected.module,
                expected.function,
                expected.function,
                expected.module
            ));
        }
        return Err(format!(
            "terlc build package `{}` requires entrypoint `{}.{}(): Unit`; found `{}` with arity {}",
            metadata.package.name,
            expected.module,
            expected.function,
            expected.function,
            arities.join(", ")
        ));
    };

    if !function.public {
        return Err(format!(
            "terlc build package `{}` entrypoint `{}.{}(): Unit` must be declared `pub`",
            metadata.package.name, expected.module, expected.function
        ));
    }

    if function.return_type != "Unit" {
        return Err(format!(
            "terlc build package `{}` entrypoint `{}.{}(): Unit` must return `Unit`, got `{}`",
            metadata.package.name, expected.module, expected.function, function.return_type
        ));
    }

    Ok(BuildEntrypoint {
        module: expected.module.clone(),
        function: expected.function.clone(),
        arity: expected.arity,
        erlang_module: crate::support::erlang_output_stem(&expected.module),
        erlang_function: expected.function.clone(),
    })
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

    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
            TargetProfileCheckOptions {
                allow_asset_imports: true,
            },
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
        compile_erlang_source(&source_dir, &ebin_dir, &runtime_path)?;
    }

    compile_erlang_source(&source_dir, &ebin_dir, &erl_path)?;

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

/// Writes the selected user-facing executable launcher.
///
/// Inputs:
/// - `out_dir`: build output root.
/// - `metadata`: manifest-derived package/build metadata.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after the executable launcher exists.
/// - `Err(message)` when directory creation, writing, or permission updates
///   fail.
///
/// Transformation:
/// - Materializes the current `beam-thin` executable contract as a single
///   launcher file under `bin/` that points Erlang at the generated `ebin`
///   directory. It does not assemble an OTP release or bundle ERTS.
fn write_build_executable_launcher(
    out_dir: &Path,
    metadata: &BuildPackageMetadata,
    entrypoint: &BuildEntrypoint,
    incremental: bool,
) -> Result<(), String> {
    match metadata.executable.mode.as_str() {
        "beam-thin" => {
            write_beam_thin_launcher(out_dir, &metadata.executable.path, entrypoint, incremental)
        }
        other => Err(format!(
            "cannot write unsupported executable artifact mode `{other}`"
        )),
    }
}

/// Writes a thin BEAM launcher script.
///
/// Inputs:
/// - `out_dir`: build output root.
/// - `relative_path`: metadata-relative executable path, such as `bin/demo`.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after the launcher exists and is executable on Unix.
/// - `Err(message)` when directory creation, writing, or permission updates
///   fail.
///
/// Transformation:
/// - Emits a portable POSIX shell launcher that resolves its own build root and
///   starts `erl` with the generated `ebin` directory on the BEAM code path.
fn write_beam_thin_launcher(
    out_dir: &Path,
    relative_path: &str,
    entrypoint: &BuildEntrypoint,
    incremental: bool,
) -> Result<(), String> {
    let executable_path = out_dir.join(relative_path);
    let parent = executable_path.parent().ok_or_else(|| {
        format!(
            "cannot resolve parent directory for executable artifact {}",
            executable_path.display()
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("cannot create build executable directory: {err}"))?;

    let script = format!(
        "#!/usr/bin/env sh\nset -eu\nSCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nROOT_DIR=$(CDPATH= cd -- \"$SCRIPT_DIR/..\" && pwd)\nexec erl -noshell -pa \"$ROOT_DIR/ebin\" -eval \"case catch {module}:{function}() of {{'EXIT', Reason}} -> io:format(standard_error, \\\"terlan entrypoint {source_module}.{source_function}/{arity} failed: ~p~n\\\", [Reason]), halt(1); _ -> halt(0) end.\" \"$@\"\n",
        module = entrypoint.erlang_module,
        function = entrypoint.erlang_function,
        source_module = entrypoint.module,
        source_function = entrypoint.function,
        arity = entrypoint.arity,
    );
    write_build_file(&executable_path, script.as_bytes(), incremental)?;
    mark_build_file_executable(&executable_path)
}

/// Marks a generated build file executable when the platform supports Unix
/// mode bits.
///
/// Inputs:
/// - `path`: generated build file path.
///
/// Output:
/// - `Ok(())` after permissions are updated or when the platform has no Unix
///   mode bits.
/// - `Err(message)` when permission reads or writes fail.
///
/// Transformation:
/// - Adds user/group/other execute bits to the generated launcher on Unix and
///   leaves non-Unix platforms to their native execution policy.
#[cfg(unix)]
fn mark_build_file_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).map_err(|err| {
        format!(
            "cannot read executable permissions {}: {err}",
            path.display()
        )
    })?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("cannot mark executable {}: {err}", path.display()))
}

/// Marks a generated build file executable when the platform supports Unix
/// mode bits.
///
/// Inputs:
/// - `path`: generated build file path.
///
/// Output:
/// - Always `Ok(())` on non-Unix platforms.
///
/// Transformation:
/// - Keeps the call site cross-platform while non-Unix executable semantics
///   remain owned by downstream target packaging.
#[cfg(not(unix))]
fn mark_build_file_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Writes package/build metadata into the build output directory.
///
/// Inputs:
/// - `out_dir`: build output root.
/// - `metadata`: manifest-derived package/build metadata.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after package metadata exists.
/// - `Err(message)` when serialization or writing fails.
///
/// Transformation:
/// - Serializes deterministic package metadata as stable JSON at
///   `terlan-package-build.json`.
fn write_build_package_metadata(
    out_dir: &Path,
    metadata: BuildPackageMetadata,
    incremental: bool,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|err| format!("failed to serialize build package metadata: {err}"))?;
    write_build_file(
        &out_dir.join(BUILD_PACKAGE_METADATA_FILE),
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
///
/// Output:
/// - `Ok(())` when `erlc` exits successfully.
/// - `Err(message)` for process spawn failures or non-zero compiler exits.
///
/// Transformation:
/// - Runs `erlc -I <source_dir> -o <ebin_dir> <erl_path>` with crash dumps
///   redirected outside the build source tree.
fn compile_erlang_source(
    source_dir: &Path,
    ebin_dir: &Path,
    erl_path: &Path,
) -> Result<(), String> {
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
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Builds a command argument vector from string slices.
    ///
    /// Inputs:
    /// - `items`: borrowed argument strings.
    ///
    /// Output:
    /// - Owned `String` vector accepted by parser helpers.
    ///
    /// Transformation:
    /// - Clones each slice into owned CLI-like arguments.
    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    /// Creates a clean temporary directory for build command tests.
    ///
    /// Inputs:
    /// - `name`: stable test-specific name segment.
    ///
    /// Output:
    /// - Path to an empty directory under the process temp directory.
    ///
    /// Transformation:
    /// - Removes any stale directory from previous runs and recreates it.
    fn make_temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("terlan_build_command_{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }

    /// Asserts that a generated file has at least one executable bit when the
    /// platform exposes Unix mode bits.
    ///
    /// Inputs:
    /// - `path`: generated file path.
    ///
    /// Output:
    /// - Test assertion success or panic.
    ///
    /// Transformation:
    /// - Reads Unix mode bits and verifies user/group/other execute permission
    ///   exists; non-Unix platforms use a no-op fallback.
    #[cfg(unix)]
    fn assert_executable_bit(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(path)
            .expect("read executable metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "launcher should be executable");
    }

    /// Asserts that a generated file has at least one executable bit when the
    /// platform exposes Unix mode bits.
    ///
    /// Inputs:
    /// - `path`: generated file path.
    ///
    /// Output:
    /// - Always succeeds on non-Unix platforms.
    ///
    /// Transformation:
    /// - Keeps launcher tests portable while execution permissions remain a
    ///   Unix-specific build artifact detail.
    #[cfg(not(unix))]
    fn assert_executable_bit(_path: &Path) {}

    #[test]
    fn parse_build_args_defaults_to_erlang_target() {
        let parsed = parse_build_args(&args(&["src/main.tl"])).expect("build args should parse");

        assert_eq!(
            parsed,
            BuildArgs {
                path: "src/main.tl".to_string(),
                target: BuildTarget::Erlang
            }
        );
    }

    #[test]
    fn parse_build_args_defaults_to_current_directory() {
        let parsed = parse_build_args(&args(&[])).expect("empty build args should parse");

        assert_eq!(
            parsed,
            BuildArgs {
                path: ".".to_string(),
                target: BuildTarget::Erlang
            }
        );
    }

    #[test]
    fn parse_build_args_accepts_explicit_erlang_target() {
        let parsed =
            parse_build_args(&args(&["src/main.tl", "--target", "erlang"])).expect("valid args");

        assert_eq!(parsed.target, BuildTarget::Erlang);
        assert_eq!(parsed.path, "src/main.tl");
    }

    #[test]
    fn parse_build_args_rejects_unsupported_target() {
        let err =
            parse_build_args(&args(&["src/main.tl", "--target", "js"])).expect_err("bad target");

        assert!(err.contains("unsupported build target `js`"));
    }

    #[test]
    fn target_profile_supports_erlang_profiles_only() {
        assert!(target_profile_supports_erlang_backend(
            TargetProfile::Erlang
        ));
        assert!(target_profile_supports_erlang_backend(
            TargetProfile::A0Erlang
        ));
        assert!(!target_profile_supports_erlang_backend(
            TargetProfile::CoreV0
        ));
    }

    #[test]
    fn build_command_emits_erlang_source_and_beam_for_single_file() {
        let dir = make_temp_dir("single_file");
        let source_path = dir.join("build_single_file.tl");
        let out_dir = dir.join("build");
        fs::write(
            &source_path,
            "module build_single_file.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
        )
        .expect("failed to write source fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_path.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/build_single_file.erl").exists());
        assert!(out_dir.join("ebin/build_single_file.beam").exists());
        assert!(
            !out_dir.join("bin/build_single_file").exists(),
            "single-file builds should not emit a package executable launcher"
        );
        assert!(
            !out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists(),
            "single-file builds should not emit package metadata"
        );

        let debug_map_text =
            fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE)).expect("read build debug map");
        let debug_map: serde_json::Value =
            serde_json::from_str(&debug_map_text).expect("parse build debug map");
        assert_eq!(debug_map["schema"], BUILD_DEBUG_MAP_SCHEMA);
        assert_eq!(debug_map["target"], "erlang");
        assert_eq!(debug_map["modules"].as_array().expect("modules").len(), 1);
        assert_eq!(debug_map["modules"][0]["module"], "build_single_file");
        assert_eq!(
            debug_map["modules"][0]["source_path"],
            source_path.to_string_lossy().to_string()
        );
        assert!(
            debug_map["modules"][0]["core_ir_hash"]
                .as_u64()
                .expect("core hash")
                > 0
        );
        assert_eq!(
            debug_map["modules"][0]["erl_path"],
            out_dir
                .join("src/build_single_file.erl")
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(
            debug_map["modules"][0]["beam_path"],
            out_dir
                .join("ebin/build_single_file.beam")
                .to_string_lossy()
                .to_string()
        );
    }

    #[test]
    fn build_command_emits_erlang_sources_and_beams_for_directory() {
        let dir = make_temp_dir("directory");
        let source_dir = dir.join("project");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            source_dir.join("a_user.tl"),
            "module a_user.\n\nimport z_dep.{add}.\n\npub value(): Int ->\n    add(1).\n",
        )
        .expect("failed to write user source fixture");
        fs::write(
            source_dir.join("z_dep.tl"),
            "module z_dep.\n\npub add(x: Int): Int ->\n    x + 1.\n",
        )
        .expect("failed to write dependency source fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/a_user.erl").exists());
        assert!(out_dir.join("src/z_dep.erl").exists());
        assert!(out_dir.join("ebin/a_user.beam").exists());
        assert!(out_dir.join("ebin/z_dep.beam").exists());

        let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
            .expect("read directory build debug map");
        let debug_map: serde_json::Value =
            serde_json::from_str(&debug_map_text).expect("parse directory build debug map");
        assert_eq!(debug_map["schema"], BUILD_DEBUG_MAP_SCHEMA);
        assert_eq!(debug_map["target"], "erlang");
        let modules = debug_map["modules"].as_array().expect("modules");
        let module_names = modules
            .iter()
            .map(|entry| entry["module"].as_str().expect("module name"))
            .collect::<Vec<_>>();
        assert_eq!(module_names, vec!["a_user", "z_dep"]);
        assert_eq!(
            modules[0]["erl_path"],
            out_dir.join("src/a_user.erl").to_string_lossy().to_string()
        );
        assert_eq!(
            modules[1]["beam_path"],
            out_dir
                .join("ebin/z_dep.beam")
                .to_string_lossy()
                .to_string()
        );
    }

    /// Verifies directory builds recursively discover package-rooted source
    /// layouts.
    ///
    /// Inputs:
    /// - A nested `std/core/Bool.tl` provider module.
    /// - A nested `app/Main.tl` consumer module importing the provider through
    ///   its dotted module identity.
    ///
    /// Output:
    /// - Test passes when `terlc build <dir> --target erlang` emits Erlang
    ///   source and BEAM artifacts for both nested source files.
    ///
    /// Transformation:
    /// - Runs recursive source discovery, directory interface-cache
    ///   validation, CoreIR lowering, Erlang source emission, and `erlc` so
    ///   package-rooted layouts are proven at artifact level.
    #[test]
    fn build_command_emits_erlang_sources_and_beams_for_recursive_package_layout() {
        let dir = make_temp_dir("directory_recursive_package_layout");
        let source_dir = dir.join("project");
        let app_dir = source_dir.join("app");
        let std_core_dir = source_dir.join("std/core");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create app source dir");
        fs::create_dir_all(&std_core_dir).expect("failed to create std core source dir");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\nimport std.core.Bool.{truth}.\n\npub value(): Bool ->\n    truth().\n",
        )
        .expect("failed to write nested app source fixture");
        fs::write(
            std_core_dir.join("Bool.tl"),
            "module std.core.Bool.\n\npub truth(): Bool ->\n    true.\n",
        )
        .expect("failed to write nested std core source fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/app_main.erl").exists());
        assert!(out_dir.join("src/std_core_bool.erl").exists());
        assert!(out_dir.join("ebin/app_main.beam").exists());
        assert!(out_dir.join("ebin/std_core_bool.beam").exists());

        let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
            .expect("read recursive directory build debug map");
        let debug_map: serde_json::Value =
            serde_json::from_str(&debug_map_text).expect("parse recursive build debug map");
        assert_eq!(debug_map["schema"], BUILD_DEBUG_MAP_SCHEMA);
        assert_eq!(debug_map["target"], "erlang");
        let modules = debug_map["modules"].as_array().expect("modules");
        let module_names = modules
            .iter()
            .map(|entry| entry["module"].as_str().expect("module name"))
            .collect::<Vec<_>>();
        assert_eq!(module_names, vec!["app.Main", "std.core.Bool"]);
        assert_eq!(
            modules[0]["source_path"],
            app_dir.join("Main.tl").to_string_lossy().to_string()
        );
        assert_eq!(
            modules[0]["erl_path"],
            out_dir
                .join("src/app_main.erl")
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(
            modules[0]["beam_path"],
            out_dir
                .join("ebin/app_main.beam")
                .to_string_lossy()
                .to_string()
        );
        assert!(
            modules[0]["core_ir_hash"].as_u64().expect("app hash") > 0,
            "app module should carry a nonzero CoreIR hash"
        );
        assert_eq!(
            modules[1]["source_path"],
            std_core_dir.join("Bool.tl").to_string_lossy().to_string()
        );
        assert_eq!(
            modules[1]["erl_path"],
            out_dir
                .join("src/std_core_bool.erl")
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(
            modules[1]["beam_path"],
            out_dir
                .join("ebin/std_core_bool.beam")
                .to_string_lossy()
                .to_string()
        );
        assert!(
            modules[1]["core_ir_hash"].as_u64().expect("std hash") > 0,
            "std.core module should carry a nonzero CoreIR hash"
        );
    }

    /// Verifies directory builds compile recursive type-only and value imports.
    ///
    /// Inputs:
    /// - A nested `std/core/UserId.tl` provider exporting a public type alias
    ///   and a public constructor-like helper function.
    /// - A nested `app/User.tl` consumer importing the provider type through
    ///   `import type` and the helper through a selected value import.
    ///
    /// Output:
    /// - Test passes when `terlc build <dir> --target erlang` emits Erlang
    ///   source and BEAM artifacts for both nested modules and records both
    ///   modules in the build debug map.
    ///
    /// Transformation:
    /// - Runs recursive directory discovery, interface-cache dependency
    ///   closure, type-only import resolution, selected value import
    ///   resolution, CoreIR lowering, Erlang source emission, and `erlc` so
    ///   package-rooted type/value import closure is proven at artifact level.
    #[test]
    fn build_command_compiles_recursive_type_and_value_import_dependency_closure() {
        let dir = make_temp_dir("directory_recursive_type_and_value_imports");
        let source_dir = dir.join("project");
        let app_dir = source_dir.join("app");
        let std_core_dir = source_dir.join("std/core");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create app source dir");
        fs::create_dir_all(&std_core_dir).expect("failed to create std core source dir");
        fs::write(
            app_dir.join("User.tl"),
            "module app.User.\n\nimport type std.core.UserId.UserId.\nimport std.core.UserId.{from_int}.\n\npub default_id(): UserId ->\n    from_int(1).\n",
        )
        .expect("failed to write recursive type/value import consumer");
        fs::write(
            std_core_dir.join("UserId.tl"),
            "module std.core.UserId.\n\npub type UserId = Int.\n\npub from_int(value: Int): UserId ->\n    value.\n",
        )
        .expect("failed to write recursive type/value import provider");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/app_user.erl").exists());
        assert!(out_dir.join("src/std_core_userid.erl").exists());
        assert!(out_dir.join("ebin/app_user.beam").exists());
        assert!(out_dir.join("ebin/std_core_userid.beam").exists());

        let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
            .expect("read type/value import directory build debug map");
        let debug_map: serde_json::Value =
            serde_json::from_str(&debug_map_text).expect("parse type/value import debug map");
        let modules = debug_map["modules"].as_array().expect("modules");
        let module_names = modules
            .iter()
            .map(|entry| entry["module"].as_str().expect("module name"))
            .collect::<Vec<_>>();
        assert_eq!(module_names, vec!["app.User", "std.core.UserId"]);
        assert_eq!(
            modules[0]["source_path"],
            app_dir.join("User.tl").to_string_lossy().to_string()
        );
        assert_eq!(
            modules[1]["source_path"],
            std_core_dir.join("UserId.tl").to_string_lossy().to_string()
        );
        assert!(
            modules
                .iter()
                .all(|entry| entry["core_ir_hash"].as_u64().expect("core hash") > 0),
            "all dependency-closure modules should carry nonzero CoreIR hashes"
        );
    }

    /// Verifies project manifests are rejected before silent source-root builds.
    ///
    /// Inputs:
    /// - A directory containing `terlan.toml` and one otherwise buildable
    ///   source module.
    ///
    /// Output:
    /// - Test passes when `terlc build <dir> --target erlang` fails and emits
    ///   no Erlang source, BEAM artifact, or build debug map.
    ///
    /// Transformation:
    /// - Runs the build command against a manifest-bearing directory and proves
    ///   A0.37 package/project manifest semantics are not silently skipped by
    ///   the plain recursive source-root build path.
    #[test]
    fn build_command_rejects_project_manifest_before_silent_directory_scan() {
        let dir = make_temp_dir("directory_project_manifest_rejected");
        let source_dir = dir.join("project");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            source_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            source_dir.join("main.tl"),
            "module main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("failed to write manifest-bearing source fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("src/main.erl").exists());
        assert!(!out_dir.join("ebin/main.beam").exists());
        assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    }

    /// Verifies project manifests build from the parsed source root.
    ///
    /// Inputs:
    /// - A project root containing `terlan.toml`.
    /// - A single manifest-declared `src` source root containing one nested
    ///   package-rooted module.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` emits Erlang
    ///   source, a BEAM artifact, and a debug-map entry for the module under
    ///   the manifest source root.
    ///
    /// Transformation:
    /// - Parses `terlan.toml`, resolves `[build] source_roots`, delegates the
    ///   selected source root to the existing formal source-root build path,
    ///   and proves the project root itself is not used as the module layout
    ///   root.
    #[test]
    fn build_command_compiles_project_manifest_source_root() {
        let dir = make_temp_dir("directory_project_manifest_source_root");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\npub main(): Unit ->\n    std.io.Console.println(\"hello\").\n",
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
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/app_main.erl").exists());
        assert!(out_dir.join("ebin/app_main.beam").exists());
        let executable_path = out_dir.join("bin/app");
        assert!(executable_path.exists());
        assert_eq!(
            fs::read_to_string(&executable_path).expect("read executable launcher"),
            "#!/usr/bin/env sh\nset -eu\nSCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nROOT_DIR=$(CDPATH= cd -- \"$SCRIPT_DIR/..\" && pwd)\nexec erl -noshell -pa \"$ROOT_DIR/ebin\" -eval \"case catch app_main:main() of {'EXIT', Reason} -> io:format(standard_error, \\\"terlan entrypoint app.Main.main/0 failed: ~p~n\\\", [Reason]), halt(1); _ -> halt(0) end.\" \"$@\"\n"
        );
        assert_executable_bit(&executable_path);
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run launcher");
        assert!(
            launcher_output.status.success(),
            "launcher failed: stdout={} stderr={}",
            String::from_utf8_lossy(&launcher_output.stdout),
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "hello\n");

        let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
            .expect("read project manifest build debug map");
        let debug_map: serde_json::Value =
            serde_json::from_str(&debug_map_text).expect("parse project manifest debug map");
        assert_eq!(debug_map["project"]["package"], "app");
        assert_eq!(debug_map["project"]["version"], "0.0.1");
        assert_eq!(debug_map["project"]["source_roots"][0], "src");
        assert_eq!(debug_map["project"]["artifact"], "beam-thin");
        let modules = debug_map["modules"].as_array().expect("modules");
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0]["module"], "app.Main");
        assert_eq!(
            modules[0]["source_path"],
            app_dir.join("Main.tl").to_string_lossy().to_string()
        );

        let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
            .expect("read project package metadata");
        let package_metadata: serde_json::Value =
            serde_json::from_str(&package_metadata_text).expect("parse project package metadata");
        assert_eq!(package_metadata["schema"], BUILD_PACKAGE_METADATA_SCHEMA);
        assert_eq!(package_metadata["target"], "erlang");
        assert_eq!(package_metadata["package"]["name"], "app");
        assert_eq!(package_metadata["package"]["version"], "0.0.1");
        assert_eq!(package_metadata["artifact"], "beam-thin");
        assert_eq!(package_metadata["executable"]["mode"], "beam-thin");
        assert_eq!(package_metadata["executable"]["path"], "bin/app");
        assert_eq!(package_metadata["executable"]["runtime"], "external-erts");
        assert_eq!(
            package_metadata["executable"]["entrypoint"]["module"],
            "app.Main"
        );
        assert_eq!(
            package_metadata["executable"]["entrypoint"]["function"],
            "main"
        );
        assert_eq!(package_metadata["executable"]["entrypoint"]["arity"], 0);
        assert_eq!(package_metadata["source_roots"][0], "src");
        assert!(
            package_metadata["dependencies"]
                .as_array()
                .expect("package dependencies")
                .is_empty(),
            "project without dependency metadata should emit an empty dependency list"
        );
    }

    /// Verifies project builds can resolve selective imports from packaged std.
    ///
    /// Inputs:
    /// - A project root without local `std/summaries` files.
    /// - A source file importing `std.io.Console.{println}` and calling the
    ///   imported function by its local name.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` succeeds and
    ///   the emitted Erlang calls the console runtime capability.
    ///
    /// Transformation:
    /// - Loads compiler-embedded std interface summaries for external project
    ///   compilation, resolves the selective import to its external target, and
    ///   lowers the target-neutral console call to Erlang `io:format`.
    #[test]
    fn build_command_resolves_selective_std_imports_from_external_project() {
        let dir = make_temp_dir("directory_project_selective_std_import");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"hello\").\n",
        )
        .expect("failed to write selective-import fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::SUCCESS);
        let erl_source =
            fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
        assert!(
            erl_source.contains("io:format"),
            "selective std import should lower to Erlang console runtime call"
        );
        assert!(
            !erl_source.contains("println(\"hello\")"),
            "selective std import should not remain an unresolved local Erlang call"
        );
        let executable_path = out_dir.join("bin/app");
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run generated project launcher");
        assert!(
            launcher_output.status.success(),
            "launcher should exit successfully: stderr={}",
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "hello\n");
    }

    /// Verifies selected std imports accept primitive receiver conversion calls.
    ///
    /// Inputs:
    /// - A project root without local `std/summaries` files.
    /// - A source file importing `std.io.Console.{println}` and calling
    ///   `println(1.to_string())`.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` emits a
    ///   launcher that prints `1`.
    ///
    /// Transformation:
    /// - Resolves the selected std import, lowers `Int.to_string` receiver
    ///   syntax through the compiler-owned primitive intrinsic path, and then
    ///   lowers `println` through the runtime console capability.
    #[test]
    fn build_command_compiles_selective_std_import_with_int_receiver_to_string() {
        let dir = make_temp_dir("directory_project_selective_std_import_int_to_string");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(1.to_string()).\n",
        )
        .expect("failed to write int receiver to_string fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::SUCCESS);
        let executable_path = out_dir.join("bin/app");
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run generated project launcher");
        assert!(
            launcher_output.status.success(),
            "launcher should exit successfully: stderr={}",
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "1\n");
    }

    /// Verifies selected primitive std function imports lower to intrinsics.
    ///
    /// Inputs:
    /// - A project root without local `std/summaries` files.
    /// - A source file importing `std.io.Console.{println}` and
    ///   `std.core.Int.{to_string}`.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` emits a
    ///   launcher that prints `2`.
    ///
    /// Transformation:
    /// - Resolves both selected std imports through compiler-embedded
    ///   interfaces, lowers `to_string(2)` through the compiler-owned primitive
    ///   intrinsic path, and lowers `println` through the runtime console
    ///   capability.
    #[test]
    fn build_command_compiles_selective_std_import_with_int_to_string_function() {
        let dir = make_temp_dir("directory_project_selective_std_import_int_function_to_string");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\nimport std.core.Int.{to_string}.\n\npub main(): Unit ->\n    println(to_string(2)).\n",
        )
        .expect("failed to write int function to_string fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::SUCCESS);
        let erl_source =
            fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
        assert!(
            erl_source.contains("erlang:integer_to_list(2)"),
            "selected primitive import should lower to Erlang intrinsic: {}",
            erl_source
        );
        let executable_path = out_dir.join("bin/app");
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run generated project launcher");
        assert!(
            launcher_output.status.success(),
            "launcher should exit successfully: stderr={}",
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "2\n");
    }

    /// Verifies imported primitive modules lower qualified calls to intrinsics.
    ///
    /// Inputs:
    /// - A project root without local `std/summaries` files.
    /// - A source file importing `std.core.Int` as a module and calling
    ///   `Int.to_string(2)`.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` emits a
    ///   launcher that prints `2`.
    ///
    /// Transformation:
    /// - Resolves the imported `Int` module alias to `std.core.Int`, recognizes
    ///   the method-shaped primitive module call, and lowers it through the
    ///   compiler-owned intrinsic path instead of emitting `int:to_string/1`.
    #[test]
    fn build_command_compiles_imported_int_module_to_string_call() {
        let dir = make_temp_dir("directory_project_imported_int_module_to_string_call");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\nimport std.core.Int.\n\npub main(): Unit ->\n    println(Int.to_string(2)).\n",
        )
        .expect("failed to write imported int module fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::SUCCESS);
        let erl_source =
            fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
        assert!(
            erl_source.contains("erlang:integer_to_list(2)"),
            "imported primitive module call should lower to Erlang intrinsic: {}",
            erl_source
        );
        assert!(
            !erl_source.contains("int:to_string"),
            "imported primitive module call must not lower to a backend module call: {}",
            erl_source
        );
        let executable_path = out_dir.join("bin/app");
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run generated project launcher");
        assert!(
            launcher_output.status.success(),
            "launcher should exit successfully: stderr={}",
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "2\n");
    }

    /// Verifies imported primitive Bool module calls lower to intrinsics.
    ///
    /// Inputs:
    /// - A project root without local `std/summaries` files.
    /// - A source file importing `std.core.Bool` as a module and calling
    ///   `Bool.to_string(true)`.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` emits a
    ///   launcher that prints `true`.
    ///
    /// Transformation:
    /// - Resolves the imported `Bool` module alias to `std.core.Bool`,
    ///   recognizes the primitive module call, and lowers it through the
    ///   compiler-owned intrinsic path instead of emitting `std_core_bool`.
    #[test]
    fn build_command_compiles_imported_bool_module_to_string_call() {
        let dir = make_temp_dir("directory_project_imported_bool_module_to_string_call");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\nimport std.core.Bool.\n\npub main(): Unit ->\n    println(Bool.to_string(true)).\n",
        )
        .expect("failed to write imported bool module fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::SUCCESS);
        let erl_source =
            fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
        assert!(
            !erl_source.contains("std_core_bool"),
            "imported primitive Bool module call must not lower to backend std module: {}",
            erl_source
        );
        let executable_path = out_dir.join("bin/app");
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run generated project launcher");
        assert!(
            launcher_output.status.success(),
            "launcher should exit successfully: stderr={}",
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "true\n");
    }

    /// Verifies selected std imports are typechecked before backend emission.
    ///
    /// Inputs:
    /// - A project root without local `std/summaries` files.
    /// - A source file importing `std.io.Console.{println}` and calling it with
    ///   an `Int` instead of the declared `String` argument.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` fails before
    ///   writing a user-facing launcher.
    ///
    /// Transformation:
    /// - Resolves the selected std import through compiler-embedded interface
    ///   summaries and proves argument mismatches are rejected by the formal
    ///   typecheck phase rather than leaking to Erlang runtime `badarg`.
    #[test]
    fn build_command_rejects_selective_std_import_argument_mismatch() {
        let dir = make_temp_dir("directory_project_selective_std_import_type_error");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(1).\n",
        )
        .expect("failed to write selective-import mismatch fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::from(1));
        assert!(
            !out_dir.join("bin/app").exists(),
            "type errors should stop before launcher emission"
        );
        assert!(
            !out_dir.join("src/app_main.erl").exists(),
            "type errors should stop before Erlang source emission"
        );
    }

    /// Verifies manifest-backed executable builds require the canonical entrypoint.
    ///
    /// Inputs:
    /// - A project manifest selecting the default `beam-thin` artifact.
    /// - A package-rooted `app.Main` module that lacks `main/0`.
    ///
    /// Output:
    /// - Test passes when the build fails and no user-facing executable
    ///   launcher or package metadata is written.
    ///
    /// Transformation:
    /// - Runs the manifest project build and proves A0.46 checks package
    ///   entrypoint shape before materializing the runnable artifact contract.
    #[test]
    fn build_command_rejects_project_manifest_without_main_entrypoint() {
        let dir = make_temp_dir("directory_project_manifest_missing_entrypoint");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
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
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("bin/app").exists());
        assert!(!out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists());
    }

    /// Verifies manifest builds lower explicit constructor declarations.
    ///
    /// Inputs:
    /// - A manifest-backed `beam-thin` project.
    /// - A package-rooted `app.Main` module with one public constructor and
    ///   one private constructor used by `main/0`.
    ///
    /// Output:
    /// - Test passes when the build emits BEAM artifacts, exports only the
    ///   public constructor helper, and the generated launcher runs `main/0`.
    ///
    /// Transformation:
    /// - Compiles explicit constructor declarations through the formal CoreIR
    ///   build path and proves constructor visibility controls the emitted
    ///   public construction API.
    #[test]
    fn build_command_compiles_project_explicit_constructor_entrypoint() {
        let dir = make_temp_dir("directory_project_explicit_constructor_entrypoint");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "\
module app.Main.\n\
\n\
pub type Done = :unit.\n\
type Hidden = :unit.\n\
\n\
pub constructor Done {\n\
    (): Done -> :unit\n\
}.\n\
\n\
constructor Hidden {\n\
    (): Hidden -> :unit\n\
}.\n\
\n\
pub main(): Unit ->\n\
    let visible = Done(); hidden = Hidden(); std.io.Console.println(\"constructors ok\").\n",
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
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        let erl_text = fs::read_to_string(out_dir.join("src/app_main.erl"))
            .expect("read generated app_main.erl");
        assert!(
            erl_text.contains("typer_ctor_done_0/0"),
            "public constructor helper should be exported and callable:\n{}",
            erl_text
        );
        assert!(
            erl_text.contains("typer_ctor_hidden_0() ->"),
            "private constructor helper should still lower for local use:\n{}",
            erl_text
        );
        assert!(
            !erl_text.contains("typer_ctor_hidden_0/0"),
            "private constructor helper must not be exported:\n{}",
            erl_text
        );

        let executable_path = out_dir.join("bin/app");
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run constructor launcher");
        assert!(
            launcher_output.status.success(),
            "launcher failed: stdout={} stderr={}",
            String::from_utf8_lossy(&launcher_output.stdout),
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&launcher_output.stdout),
            "constructors ok\n"
        );
    }

    /// Verifies manifest builds lower general receiver-method dispatch.
    ///
    /// Inputs:
    /// - A manifest-backed `beam-thin` project.
    /// - A package-rooted `app.Main` module with a struct, a receiver method,
    ///   and an executable entrypoint that invokes the method through
    ///   `receiver.method()`.
    ///
    /// Output:
    /// - Test passes when the build emits BEAM artifacts, rewrites the method
    ///   call to the receiver-first backend convention, and the generated
    ///   launcher prints the method result.
    ///
    /// Transformation:
    /// - Compiles local receiver-method dispatch through the formal
    ///   syntax-output/typecheck/build path and proves the compatibility
    ///   Erlang backend can execute the lowered method call.
    #[test]
    fn build_command_compiles_project_receiver_method_entrypoint() {
        let dir = make_temp_dir("directory_project_receiver_method_entrypoint");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "\
module app.Main.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub constructor User {\n\
    (name: String): User -> #User{ name = name }\n\
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
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        let erl_text = fs::read_to_string(out_dir.join("src/app_main.erl"))
            .expect("read generated app_main.erl");
        assert!(
            erl_text.contains("display_name("),
            "receiver method should lower as a receiver-first function:\n{}",
            erl_text
        );

        let executable_path = out_dir.join("bin/app");
        let launcher_output = Command::new(&executable_path)
            .output()
            .expect("run receiver-method launcher");
        assert!(
            launcher_output.status.success(),
            "launcher failed: stdout={} stderr={}",
            String::from_utf8_lossy(&launcher_output.stdout),
            String::from_utf8_lossy(&launcher_output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Ada\n");
    }

    /// Verifies manifest-backed builds reject source files outside the package root.
    ///
    /// Inputs:
    /// - A project manifest whose package name is `app`.
    /// - A source file under `src/other` declaring `module other.Main`.
    ///
    /// Output:
    /// - Test passes when build fails before writing Erlang source, BEAM
    ///   artifacts, debug maps, package metadata, or executable launchers.
    ///
    /// Transformation:
    /// - Runs the project build path and proves manifest package identity is
    ///   enforced before the existing source-root layout and backend gates.
    #[test]
    fn build_command_rejects_project_source_outside_package_root() {
        let dir = make_temp_dir("directory_project_manifest_package_root_mismatch");
        let project_dir = dir.join("project");
        let other_dir = project_dir.join("src/other");
        let out_dir = dir.join("build");
        fs::create_dir_all(&other_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            other_dir.join("Main.tl"),
            "module other.Main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("failed to write mismatched package-root module");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("src/other_main.erl").exists());
        assert!(!out_dir.join("ebin/other_main.beam").exists());
        assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
        assert!(!out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists());
        assert!(!out_dir.join("bin/app").exists());
    }

    /// Verifies Erlang package adapter metadata remains metadata-only.
    ///
    /// Inputs:
    /// - A project manifest reserving the Rebar3-compatible Erlang packaging
    ///   adapter.
    /// - A buildable manifest source root.
    ///
    /// Output:
    /// - Test passes when the build succeeds, records adapter metadata in
    ///   `terlan-package-build.json`, and does not generate Rebar3 files.
    ///
    /// Transformation:
    /// - Parses `[target.erlang.package]`, runs the formal project build path,
    ///   and proves A0.42.6 preserves adapter intent without making Rebar3 part
    ///   of normal `terlc build --target erlang`.
    #[test]
    fn build_command_preserves_erlang_package_adapter_metadata_without_rebar3_files() {
        let dir = make_temp_dir("directory_project_manifest_erlang_package_adapter");
        let project_dir = dir.join("project");
        let app_dir = project_dir.join("src/app");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_dir).expect("failed to create project src dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[target.erlang.package]\nadapter = \"rebar3-compatible\"\n",
        )
        .expect("failed to write project manifest fixture");
        fs::write(
            app_dir.join("Main.tl"),
            "module app.Main.\n\npub main(): Unit ->\n    :unit.\n",
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
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/app_main.erl").exists());
        assert!(out_dir.join("ebin/app_main.beam").exists());
        assert!(
            !out_dir.join("rebar.config").exists(),
            "adapter metadata must not generate Rebar3 files in A0.42.6"
        );
        assert!(
            !out_dir.join("src/demo.app.src").exists(),
            "adapter metadata must not generate OTP app metadata in A0.42.6"
        );

        let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
            .expect("read project package metadata");
        let package_metadata: serde_json::Value =
            serde_json::from_str(&package_metadata_text).expect("parse project package metadata");
        let adapters = package_metadata["adapters"].as_array().expect("adapters");
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0]["target"], "erlang");
        assert_eq!(adapters[0]["adapter"], "rebar3-compatible");
    }

    /// Verifies project manifests build multiple declared source roots.
    ///
    /// Inputs:
    /// - A project root containing `terlan.toml`.
    /// - Two manifest-declared source roots where the second imports a value
    ///   from the first.
    ///
    /// Output:
    /// - Test passes when `terlc build <project> --target erlang` emits Erlang
    ///   sources, BEAM artifacts, and one combined debug map for both roots.
    ///
    /// Transformation:
    /// - Parses `terlan.toml`, resolves all `[build] source_roots`, validates
    ///   each root with a shared interface cache, lowers both roots through
    ///   CoreIR, and writes one source-to-artifact map across the project.
    #[test]
    fn build_command_compiles_project_manifest_multiple_source_roots() {
        let dir = make_temp_dir("directory_project_manifest_multiple_source_roots");
        let project_dir = dir.join("project");
        let lib_dir = project_dir.join("lib/demo");
        let app_dir = project_dir.join("app/demo");
        let out_dir = dir.join("build");
        fs::create_dir_all(&lib_dir).expect("failed to create project lib dir");
        fs::create_dir_all(&app_dir).expect("failed to create project app dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"lib\", \"app\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write multi-root project manifest fixture");
        fs::write(
            lib_dir.join("Util.tl"),
            "module demo.Util.\n\npub one(): Int ->\n    1.\n",
        )
        .expect("failed to write multi-root provider module");
        fs::write(
            app_dir.join("Main.tl"),
            "module demo.Main.\n\nimport demo.Util.{one}.\n\npub main(): Unit ->\n    :unit.\n\npub value(): Int ->\n    one().\n",
        )
        .expect("failed to write multi-root consumer module");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/demo_util.erl").exists());
        assert!(out_dir.join("src/demo_main.erl").exists());
        assert!(out_dir.join("ebin/demo_util.beam").exists());
        assert!(out_dir.join("ebin/demo_main.beam").exists());

        let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
            .expect("read multi-root project build debug map");
        let debug_map: serde_json::Value =
            serde_json::from_str(&debug_map_text).expect("parse multi-root project debug map");
        assert_eq!(debug_map["project"]["package"], "demo");
        assert_eq!(debug_map["project"]["version"], "0.0.1");
        assert_eq!(debug_map["project"]["source_roots"][0], "lib");
        assert_eq!(debug_map["project"]["source_roots"][1], "app");
        assert_eq!(debug_map["project"]["artifact"], "beam-thin");
        let modules = debug_map["modules"].as_array().expect("modules");
        let module_names = modules
            .iter()
            .map(|entry| entry["module"].as_str().expect("module name"))
            .collect::<Vec<_>>();
        assert_eq!(module_names, vec!["demo.Util", "demo.Main"]);
        assert_eq!(
            modules[0]["source_path"],
            lib_dir.join("Util.tl").to_string_lossy().to_string()
        );
        assert_eq!(
            modules[1]["source_path"],
            app_dir.join("Main.tl").to_string_lossy().to_string()
        );
    }

    /// Verifies project builds include local path dependency source roots.
    ///
    /// Inputs:
    /// - A root project manifest with a local `[dependencies]` path entry.
    /// - A dependency project with its own manifest and source root.
    /// - A root source file that imports a value from the dependency source.
    ///
    /// Output:
    /// - Test passes when both dependency and root modules emit Erlang source
    ///   and BEAM artifacts through one project build.
    ///
    /// Transformation:
    /// - Resolves the local path dependency manifest before backend emission,
    ///   validates dependency source roots before the root source root, and
    ///   emits the ordered package closure through the existing build path.
    #[test]
    fn build_command_compiles_project_with_local_path_dependency() {
        let dir = make_temp_dir("project_local_path_dependency");
        let app_dir = dir.join("app");
        let dep_dir = dir.join("local_utils");
        let app_src = app_dir.join("src/app");
        let dep_src = dep_dir.join("src/local_utils");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_src).expect("failed to create app src dir");
        fs::create_dir_all(&dep_src).expect("failed to create dependency src dir");
        fs::write(
            app_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n",
        )
        .expect("failed to write app manifest");
        fs::write(
            dep_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"local_utils\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("failed to write dependency manifest");
        fs::write(
            dep_src.join("Util.tl"),
            "module local_utils.Util.\n\npub one(): Int ->\n    1.\n",
        )
        .expect("failed to write dependency module");
        fs::write(
            app_src.join("Main.tl"),
            "module app.Main.\n\nimport local_utils.Util.{one}.\n\npub main(): Unit ->\n    :unit.\n\npub value(): Int ->\n    one().\n",
        )
        .expect("failed to write app module");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                app_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/local_utils_util.erl").exists());
        assert!(out_dir.join("src/app_main.erl").exists());
        assert!(out_dir.join("ebin/local_utils_util.beam").exists());
        assert!(out_dir.join("ebin/app_main.beam").exists());

        let debug_map_text = fs::read_to_string(out_dir.join(BUILD_DEBUG_MAP_FILE))
            .expect("read local dependency project debug map");
        let debug_map: serde_json::Value =
            serde_json::from_str(&debug_map_text).expect("parse local dependency debug map");
        let modules = debug_map["modules"].as_array().expect("modules");
        let module_names = modules
            .iter()
            .map(|entry| entry["module"].as_str().expect("module name"))
            .collect::<Vec<_>>();
        assert_eq!(module_names, vec!["local_utils.Util", "app.Main"]);

        let package_metadata_text = fs::read_to_string(out_dir.join(BUILD_PACKAGE_METADATA_FILE))
            .expect("read local dependency package metadata");
        let package_metadata: serde_json::Value = serde_json::from_str(&package_metadata_text)
            .expect("parse local dependency package metadata");
        assert_eq!(package_metadata["schema"], BUILD_PACKAGE_METADATA_SCHEMA);
        assert_eq!(package_metadata["package"]["name"], "app");
        let dependencies = package_metadata["dependencies"]
            .as_array()
            .expect("package dependencies");
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0]["alias"], "local_utils");
        assert_eq!(dependencies[0]["scope"], "local");
        assert_eq!(dependencies[0]["source"], "path");
        assert_eq!(dependencies[0]["path"], "../local_utils");
        assert!(dependencies[0].get("package").is_none());
        assert!(dependencies[0].get("version").is_none());
    }

    /// Verifies local path dependencies require their own manifest.
    ///
    /// Inputs:
    /// - A root project with a local `path` dependency.
    /// - A dependency directory without `terlan.toml`.
    ///
    /// Output:
    /// - Test passes when build fails before generated artifacts are written.
    ///
    /// Transformation:
    /// - Resolves local dependency metadata, checks for the dependency
    ///   manifest, and rejects the project before source-root validation or
    ///   backend emission can run.
    #[test]
    fn build_command_rejects_local_path_dependency_without_manifest() {
        let dir = make_temp_dir("project_local_path_dependency_missing_manifest");
        let app_dir = dir.join("app");
        let dep_dir = dir.join("local_utils");
        let app_src = app_dir.join("src");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_src).expect("failed to create app src dir");
        fs::create_dir_all(&dep_dir).expect("failed to create dependency dir");
        fs::write(
            app_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n",
        )
        .expect("failed to write app manifest");
        fs::write(
            app_src.join("main.tl"),
            "module main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("failed to write app module");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                app_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("src/main.erl").exists());
        assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    }

    /// Verifies local path dependency cycles fail before backend emission.
    ///
    /// Inputs:
    /// - Two project manifests that depend on each other through local `path`
    ///   dependencies.
    ///
    /// Output:
    /// - Test passes when the build fails and no backend artifacts are written.
    ///
    /// Transformation:
    /// - Tracks packages currently being resolved and rejects a dependency path
    ///   that re-enters the active resolution stack.
    #[test]
    fn build_command_rejects_local_path_dependency_cycle() {
        let dir = make_temp_dir("project_local_path_dependency_cycle");
        let app_dir = dir.join("app");
        let dep_dir = dir.join("local_utils");
        let app_src = app_dir.join("src");
        let dep_src = dep_dir.join("src");
        let out_dir = dir.join("build");
        fs::create_dir_all(&app_src).expect("failed to create app src dir");
        fs::create_dir_all(&dep_src).expect("failed to create dependency src dir");
        fs::write(
            app_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[dependencies]\nlocal_utils = { path = \"../local_utils\" }\n",
        )
        .expect("failed to write app manifest");
        fs::write(
            dep_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"local_utils\"\nversion = \"0.0.1\"\n\n[dependencies]\napp = { path = \"../app\" }\n",
        )
        .expect("failed to write dependency manifest");
        fs::write(
            app_src.join("main.tl"),
            "module main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("failed to write app module");
        fs::write(
            dep_src.join("util.tl"),
            "module util.\n\npub one(): Int ->\n    1.\n",
        )
        .expect("failed to write dependency module");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                app_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("src/main.erl").exists());
        assert!(!out_dir.join("src/util.erl").exists());
        assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    }

    /// Verifies Hex dependency metadata is rejected before backend emission.
    ///
    /// Inputs:
    /// - A project manifest with `[target.erlang.dependencies]`.
    /// - A buildable source root.
    ///
    /// Output:
    /// - Test passes when build exits with failure and writes no artifacts.
    ///
    /// Transformation:
    /// - Parses the target-scoped dependency metadata, detects unsupported Hex
    ///   package-manager integration, and stops before source-root emission.
    #[test]
    fn build_command_rejects_hex_dependency_metadata_before_emission() {
        let dir = make_temp_dir("project_hex_dependency_metadata");
        let project_dir = dir.join("project");
        let source_dir = project_dir.join("src");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.erlang.dependencies]\ncowboy = { hex = \"cowboy\", version = \"2.12.0\" }\n",
        )
        .expect("failed to write project manifest");
        fs::write(
            source_dir.join("main.tl"),
            "module main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("failed to write project module");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("src/main.erl").exists());
        assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    }

    /// Verifies npm dependency metadata is rejected before backend emission.
    ///
    /// Inputs:
    /// - A project manifest with `[target.js.dependencies]`.
    /// - A buildable source root.
    ///
    /// Output:
    /// - Test passes when build exits with failure and writes no artifacts.
    ///
    /// Transformation:
    /// - Parses the target-scoped dependency metadata, detects unsupported npm
    ///   package-manager integration, and stops before source-root emission.
    #[test]
    fn build_command_rejects_npm_dependency_metadata_before_emission() {
        let dir = make_temp_dir("project_npm_dependency_metadata");
        let project_dir = dir.join("project");
        let source_dir = project_dir.join("src");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.js.dependencies]\nzod = { npm = \"zod\", version = \"3.25.0\" }\n",
        )
        .expect("failed to write project manifest");
        fs::write(
            source_dir.join("main.tl"),
            "module main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("failed to write project module");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("src/main.erl").exists());
        assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    }

    /// Verifies cargo dependency metadata is rejected before backend emission.
    ///
    /// Inputs:
    /// - A project manifest with `[target.rust.dependencies]`.
    /// - A buildable source root.
    ///
    /// Output:
    /// - Test passes when build exits with failure and writes no artifacts.
    ///
    /// Transformation:
    /// - Parses the target-scoped dependency metadata, detects unsupported
    ///   Cargo package-manager integration, and stops before source-root
    ///   emission.
    #[test]
    fn build_command_rejects_cargo_dependency_metadata_before_emission() {
        let dir = make_temp_dir("project_cargo_dependency_metadata");
        let project_dir = dir.join("project");
        let source_dir = project_dir.join("src");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[target.rust.dependencies]\nserde = { cargo = \"serde\", version = \"1.0.0\" }\n",
        )
        .expect("failed to write project manifest");
        fs::write(
            source_dir.join("main.tl"),
            "module main.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("failed to write project module");

        let state = CliState {
            out_dir: out_dir.clone(),
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

        assert_eq!(status, ExitCode::from(1));
        assert!(!out_dir.join("src/main.erl").exists());
        assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    }

    #[test]
    fn build_command_compiles_directory_with_imported_constructors_and_aliases() {
        let dir = make_temp_dir("directory_imported_constructors");
        let source_dir = dir.join("project");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            source_dir.join("a_user.tl"),
            "module a_user.\n\nimport z_shapes.{Box, Ok}.\n\npub make_box(value: Int): Dynamic ->\n    Box(value).\n\npub make_ok(value: Int): Dynamic ->\n    Ok(value).\n",
        )
        .expect("failed to write constructor user source fixture");
        fs::write(
            source_dir.join("z_shapes.tl"),
            "module z_shapes.\n\npub type Ok[T] =\n    {:ok, value: T}.\n\npub constructor Box {\n    (value: Int): Dynamic ->\n        {:box, value}\n}.\n",
        )
        .expect("failed to write constructor provider source fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/a_user.erl").exists());
        assert!(out_dir.join("src/z_shapes.erl").exists());
        assert!(out_dir.join("ebin/a_user.beam").exists());
        assert!(out_dir.join("ebin/z_shapes.beam").exists());
    }

    /// Verifies directory builds compile aliased imported constructor-like
    /// aliases in expression and pattern positions.
    ///
    /// Inputs:
    /// - A provider module exporting a single-shape `Ok[T]` alias.
    /// - A consumer module importing `Ok as Success`, constructing
    ///   `Success(value)`, and matching `Success(value)` in a `case`.
    ///
    /// Output:
    /// - Test passes when `terlc build <dir> --target erlang` emits Erlang
    ///   source and BEAM artifacts for both modules.
    ///
    /// Transformation:
    /// - Runs the formal directory build path through interface-cache
    ///   validation, CoreIR lowering, Erlang source emission, and `erlc` so
    ///   aliased imported alias identities are proven at artifact level.
    #[test]
    fn build_command_compiles_directory_with_aliased_imported_alias_patterns() {
        let dir = make_temp_dir("directory_aliased_imported_alias_patterns");
        let source_dir = dir.join("project");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            source_dir.join("a_user.tl"),
            "module a_user.\n\nimport z_result.{Ok as Success}.\n\npub make_success(value: Int): Dynamic ->\n    Success(value).\n\npub unwrap_success(input: Dynamic): Dynamic ->\n    case input {\n        Success(value) -> value;\n        _ -> 0\n    }.\n",
        )
        .expect("failed to write aliased alias user source fixture");
        fs::write(
            source_dir.join("z_result.tl"),
            "module z_result.\n\npub type Ok[T] =\n    {:ok, value: T}.\n",
        )
        .expect("failed to write aliased alias provider source fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/a_user.erl").exists());
        assert!(out_dir.join("src/z_result.erl").exists());
        assert!(out_dir.join("ebin/a_user.beam").exists());
        assert!(out_dir.join("ebin/z_result.beam").exists());
    }

    /// Verifies directory builds compile aliased imported constructor-like
    /// aliases used as constructor-chain bases.
    ///
    /// Inputs:
    /// - A provider module exporting a single-shape `User` alias.
    /// - A consumer module importing `User as Member` and using
    ///   `Member(id, name) with Admin { ... }`.
    ///
    /// Output:
    /// - Test passes when `terlc build <dir> --target erlang` emits Erlang
    ///   source and BEAM artifacts for both modules.
    ///
    /// Transformation:
    /// - Runs the formal directory build path through interface-cache
    ///   validation, constructor-chain identity resolution, CoreIR lowering,
    ///   Erlang source emission, and `erlc` so aliased imported constructor
    ///   chains are proven at artifact level.
    #[test]
    fn build_command_compiles_directory_with_aliased_imported_alias_constructor_chain() {
        let dir = make_temp_dir("directory_aliased_imported_alias_constructor_chain");
        let source_dir = dir.join("project");
        let out_dir = dir.join("build");
        fs::create_dir_all(&source_dir).expect("failed to create source dir");
        fs::write(
            source_dir.join("a_user.tl"),
            "module a_user.\n\nimport z_user.{User as Member}.\n\npub make_admin(id: Int, name: Binary): Dynamic ->\n    Member(id, name) with Admin { id = id, name = name }.\n",
        )
        .expect("failed to write aliased alias constructor-chain user source fixture");
        fs::write(
            source_dir.join("z_user.tl"),
            "module z_user.\n\npub type User =\n    {:user, id: Int, name: Binary}.\n",
        )
        .expect("failed to write aliased alias constructor-chain provider source fixture");

        let state = CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        };
        let cmd = CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.display().to_string(),
                "--target".to_string(),
                "erlang".to_string(),
            ],
        };

        let status = run(cmd, state);

        assert_eq!(status, ExitCode::SUCCESS);
        assert!(out_dir.join("src/a_user.erl").exists());
        assert!(out_dir.join("src/z_user.erl").exists());
        assert!(out_dir.join("ebin/a_user.beam").exists());
        assert!(out_dir.join("ebin/z_user.beam").exists());
    }
}
