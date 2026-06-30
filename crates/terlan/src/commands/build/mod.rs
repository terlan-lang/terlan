use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use crate::commands::artifacts::{
    collect_syntax_file_import_bytes, collect_syntax_markdown_inputs,
    collect_syntax_template_inputs, fingerprint,
};
use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::terlan_erlang::{
    emit_html_runtime_to_erlang, emit_native_bridge_runtime_to_erlang,
    emit_native_vector_runtime_to_erlang, emit_sql_runtime_to_erlang,
    try_emit_core_module_to_erlang_with_syntax_bridge, try_emit_syntax_struct_headers_to_hrl,
};
use crate::validation::native_policy::{source_uses_native, NativePolicy};
use crate::validation::target_profile::TargetProfileCheckOptions;
use crate::{CliCommand, CliState};

mod args;
mod erlang_compile;
mod js;
mod js_assets;
mod js_browser;
mod js_model;
mod js_source_classification;
mod metadata;
mod package_artifact;
mod package_layout;
mod project_roots;
mod source_roots;
mod target_gate;
mod wasm_model;

use args::{parse_build_args, BuildArgs, BuildTarget};
use erlang_compile::compile_erlang_source;
#[cfg(test)]
use erlang_compile::erlang_source_compile_is_current;
use package_artifact::{
    validate_build_entrypoint, write_build_executable_launcher, write_build_package_metadata,
};
use package_layout::validate_project_source_package_root;
use project_roots::{reject_unsupported_external_dependencies, resolve_project_build_roots};
use source_roots::{run_erlang_project_source_roots_build, run_erlang_source_root_build};
use target_gate::{
    reject_erlang_native_package_source, reject_unsupported_target_std_source,
    target_profile_supports_erlang_backend,
};

use metadata::{
    build_package_metadata, BuildDebugMap, BuildDebugModuleEntry, BuildDebugProject,
    BuildEntrypointFunction, BuildModuleArtifact, BuildPackageMetadata, ProjectSourceRoot,
};

pub(crate) mod project_manifest;

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

    if compiled_module_uses_beam_native_bridge(compiled) {
        let runtime_path = source_dir.join("terlan_native_bridge_runtime.erl");
        write_build_file(
            &runtime_path,
            emit_native_bridge_runtime_to_erlang().as_bytes(),
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

/// Returns whether compiled artifacts need the NativeBridge runtime boundary.
///
/// Inputs:
/// - `compiled`: checked compiler artifacts for one source module.
///
/// Output:
/// - `true` when the module imports `std.beam.NativeBridge`.
///
/// Transformation:
/// - Uses resolved CoreIR imports so helper emission follows the same
///   compiler-owned target-profile and backend lowering decisions as the
///   generated module.
fn compiled_module_uses_beam_native_bridge(compiled: &CheckedSyntaxModuleArtifacts) -> bool {
    compiled
        .core
        .imports
        .iter()
        .any(|import| import.module == "std.beam.NativeBridge")
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

#[cfg(test)]
mod build_test;
