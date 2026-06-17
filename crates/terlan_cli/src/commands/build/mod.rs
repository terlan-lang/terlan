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
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    executable: Option<BuildPackageExecutable>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    features: Option<Vec<String>>,
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
    package_path: Vec<String>,
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
            project_manifest::ProjectDependencySource::Cargo {
                package, version, ..
            },
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
                package_path: source_package_path(&manifest.package),
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
            namespace: manifest.package.namespace.clone(),
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
            namespace: manifest.package.namespace.clone(),
        },
        artifact: manifest.artifact.as_str().to_string(),
        executable: build_package_executable_metadata(manifest),
        source_roots: manifest.source_roots.clone(),
        dependencies,
        adapters: build_package_adapter_metadata(manifest),
    }
}

/// Builds deterministic executable artifact metadata when the artifact is runnable.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Serializable executable artifact metadata for runnable artifact modes.
/// - `None` for library artifact modes.
///
/// Transformation:
/// - Converts `beam-thin` into launcher metadata and treats `library` as a
///   non-executable package artifact.
fn build_package_executable_metadata(
    manifest: &project_manifest::ProjectManifest,
) -> Option<BuildPackageExecutable> {
    match manifest.artifact {
        project_manifest::ProjectArtifactKind::BeamThin => Some(BuildPackageExecutable {
            mode: "beam-thin".to_string(),
            path: format!("bin/{}", manifest.package.name),
            runtime: "external-erts".to_string(),
            entrypoint: BuildPackageEntrypoint {
                module: format!("{}.Main", source_package_module_prefix(&manifest.package)),
                function: "main".to_string(),
                arity: 0,
            },
        }),
        project_manifest::ProjectArtifactKind::Library => None,
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
            features: None,
        },
        project_manifest::ProjectDependencySource::Hex { package, version } => {
            BuildPackageDependency {
                alias: dependency.alias.clone(),
                scope: package_dependency_scope(&dependency.scope).to_string(),
                source: "hex".to_string(),
                path: None,
                package: Some(package.clone()),
                version: Some(version.clone()),
                features: None,
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
                features: None,
            }
        }
        project_manifest::ProjectDependencySource::Cargo {
            package,
            version,
            features,
        } => BuildPackageDependency {
            alias: dependency.alias.clone(),
            scope: package_dependency_scope(&dependency.scope).to_string(),
            source: "cargo".to_string(),
            path: None,
            package: Some(package.clone()),
            version: Some(version.clone()),
            features: (!features.is_empty()).then(|| features.clone()),
        },
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
            eprintln!(
                "terlc build found no .terl files in {}",
                root.path.display()
            );
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

/// Converts a package identity into source namespace path segments.
///
/// Inputs:
/// - `package`: manifest `[package]` identity.
///
/// Output:
/// - Lowercase module path segments used in source layout validation.
///
/// Transformation:
/// - Uses explicit `[package] namespace` when present, otherwise derives one
///   segment from the package name by replacing package-manager dashes with
///   underscores.
fn source_package_path(package: &project_manifest::ProjectPackage) -> Vec<String> {
    package
        .namespace
        .as_deref()
        .map(|namespace| namespace.split('.').map(str::to_string).collect())
        .unwrap_or_else(|| vec![source_package_root(&package.name)])
}

/// Converts a package identity into a dotted source module prefix.
///
/// Inputs:
/// - `package`: manifest `[package]` identity.
///
/// Output:
/// - Dotted module prefix used for executable entrypoint conventions.
///
/// Transformation:
/// - Joins `source_package_path` using Terlan module path dots.
fn source_package_module_prefix(package: &project_manifest::ProjectPackage) -> String {
    source_package_path(package).join(".")
}

/// Converts a package name into the default source module root spelling.
///
/// Inputs:
/// - `package_name`: manifest `[package] name` value.
///
/// Output:
/// - Lowercase module-root spelling used when no explicit namespace is set.
///
/// Transformation:
/// - Replaces package-manager dashes with underscores because Terlan module
///   path segments use `LowerIdent`, while package names may contain `-`.
fn source_package_root(package_name: &str) -> String {
    package_name.replace('-', "_")
}

/// Validates that a manifest source file starts under the package namespace.
///
/// Inputs:
/// - `source_root`: manifest-declared source root.
/// - `file`: discovered Terlan source file under the source root.
/// - `package_path`: normalized package namespace expected as the leading
///   relative source path segments.
///
/// Output:
/// - `Ok(())` when the file path starts with the package namespace.
/// - `Err(message)` when the file is outside the root, contains non-UTF-8
///   path segments, or has a different namespace prefix.
///
/// Transformation:
/// - Checks source-root-relative paths before the existing `terlc check <dir>`
///   pass validates the full module declaration against that path.
fn validate_project_source_package_root(
    source_root: &Path,
    file: &Path,
    package_path: &[String],
) -> Result<(), String> {
    let relative = file.strip_prefix(source_root).map_err(|_| {
        format!(
            "source file `{}` is not under project source root `{}`",
            file.display(),
            source_root.display()
        )
    })?;
    let actual = relative
        .components()
        .take(package_path.len())
        .map(|component| {
            component.as_os_str().to_str().ok_or_else(|| {
                format!(
                    "source path `{}` contains a non-UTF-8 package namespace segment",
                    file.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected = package_path.iter().map(String::as_str).collect::<Vec<_>>();
    if actual == expected {
        return Ok(());
    }
    let expected_path = package_path.join("/");
    Err(format!(
        "project source file `{}` is outside package namespace `{}`; expected path under `{}/{}`",
        file.display(),
        package_path.join("."),
        source_root.display(),
        expected_path
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
    let expected = &metadata
        .executable
        .as_ref()
        .expect("entrypoint validation requires executable metadata")
        .entrypoint;
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
    if let Err(message) = reject_erlang_native_package_source(path, &source) {
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

/// Rejects native package source on the Erlang backend.
///
/// Inputs:
/// - `path`: source path used for diagnostics.
/// - `source`: Terlan source text to inspect before formal lowering.
///
/// Output:
/// - `Ok(())` when the source does not declare or import `std.native.*`.
/// - `Err(String)` with a stable target-capability diagnostic when native
///   package syntax is present.
///
/// Transformation:
/// - Performs a conservative textual boundary check before Erlang emission so
///   native package modules and consumers fail with a target-neutral capability
///   message instead of leaking unresolved imports or backend-specific errors.
fn reject_erlang_native_package_source(path: &str, source: &str) -> Result<(), String> {
    if source.contains("module std.native.") {
        return Err(format!(
            "terlc build --target erlang cannot compile native package module `{path}`; `std.native` packages require the Rust/native target capability"
        ));
    }
    if source.contains("import std.native.") || source.contains("import type std.native.") {
        return Err(format!(
            "terlc build --target erlang cannot import native package from `{path}`; `std.native` packages require the Rust/native target capability"
        ));
    }
    Ok(())
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
    executable: &BuildPackageExecutable,
    entrypoint: &BuildEntrypoint,
    incremental: bool,
) -> Result<(), String> {
    match executable.mode.as_str() {
        "beam-thin" => write_beam_thin_launcher(out_dir, &executable.path, entrypoint, incremental),
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
mod build_test;
