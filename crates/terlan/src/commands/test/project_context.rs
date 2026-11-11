use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};
use crate::CliState;

use super::arguments::{TestArgs, TEST_SOURCE_PATTERN_DESCRIPTION};
use super::execution::TestProjectContext;
use super::suite::{run_js_test_files, run_terlan_vm_test_files};

/// Validates all JavaScript test modules below one directory.
///
/// Inputs:
/// - `args`: parsed test arguments whose path is a directory.
/// - `state`: global CLI state used for formal compilation.
/// - `profile`: effective JavaScript target profile for every file.
///
/// Output:
/// - `ExitCode::SUCCESS` when every discovered JS test file validates.
/// - `ExitCode::from(1)` when discovery fails, no test files exist, manifest
///   flags are used with a directory, or any test file fails.
///
/// Transformation:
/// - Recursively discovers `*Test.terl` or legacy `*_test.terl` files in
///   deterministic order, then validates each owning project once and reuses
///   content-addressed validation evidence for unchanged modules.
pub(super) fn run_js_test_directory(
    args: &TestArgs,
    state: CliState,
    profile: TargetProfile,
) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!(
            "test manifest output is only supported for a single {TEST_SOURCE_PATTERN_DESCRIPTION} file"
        );
        return ExitCode::from(1);
    }

    let mut files = Vec::new();
    if let Err(message) = collect_test_files(Path::new(&args.path), &mut files) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    files.sort();
    if files.is_empty() {
        eprintln!(
            "no {TEST_SOURCE_PATTERN_DESCRIPTION} files found in {}",
            args.path
        );
        return ExitCode::from(1);
    }

    run_js_test_files(args, state, profile, files)
}

/// Executes all test modules below one directory through the Terlan VM.
///
/// Inputs:
/// - `args`: parsed test arguments whose path is a directory.
/// - `state`: global CLI state used for formal compilation and execution.
///
/// Output:
/// - `ExitCode::SUCCESS` when every discovered test file passes in the VM.
/// - `ExitCode::from(1)` when discovery fails, no test files exist, manifest
///   flags are used with a directory, or any file fails.
///
/// Transformation:
/// - Recursively discovers `*Test.terl` or legacy `*_test.terl` files in
///   deterministic order, then compiles the complete directory as shared
///   project suites instead of rebuilding the project once per test file.
pub(super) fn run_terlan_vm_test_directory(args: &TestArgs, state: CliState) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!(
            "test manifest output is only supported for a single {TEST_SOURCE_PATTERN_DESCRIPTION} file"
        );
        return ExitCode::from(1);
    }

    let mut files = Vec::new();
    if let Err(message) = collect_test_files(Path::new(&args.path), &mut files) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    files.sort();
    if files.is_empty() {
        eprintln!(
            "no {TEST_SOURCE_PATTERN_DESCRIPTION} files found in {}",
            args.path
        );
        return ExitCode::from(1);
    }

    run_terlan_vm_test_files(args, state, files, true)
}

/// Prepares manifest source roots for one test file.
///
/// Inputs:
/// - `path`: active test file passed to `terlc test`.
/// - `state`: command state before project-specific cache selection.
/// - `profile`: profile used to validate project source roots.
/// - `exhaustive_project`: whether every project implementation must typecheck.
///
/// Output:
/// - Optional project context and state with an effective cache directory.
/// - Error text when a discovered project manifest or source root is invalid.
///
/// Transformation:
/// - Walks upward from the test file to find `terlan.toml`; when found, reads
///   `[build] source_roots`, prepares their interfaces, and uses the same cache
///   for subsequent test compilation. Directory runs also typecheck every
///   implementation; focused files defer implementation checks to their
///   reachable compilation closure.
pub(super) fn prepare_test_project_context(
    path: &str,
    state: CliState,
    profile: TargetProfile,
    exhaustive_project: bool,
) -> Result<(Option<TestProjectContext>, CliState), String> {
    let Some(context) = discover_test_project_context(Path::new(path), state.cache_dir.as_ref())?
    else {
        return Ok((None, state));
    };

    let mut project_state = state.clone();
    project_state.target_profile = profile;
    project_state.cache_dir = Some(context.cache_dir.clone());
    project_state.incremental = true;

    for root in &context.compilation_source_roots {
        if exhaustive_project {
            let status = crate::commands::check::run_check_dir(
                &root.to_string_lossy(),
                project_state.clone(),
                None,
            );
            if status != ExitCode::SUCCESS {
                return Err(format!(
                    "project source root `{}` failed while preparing tests",
                    root.display()
                ));
            }
        } else if let Err(message) =
            crate::commands::build::prepare_source_root_interfaces(root, &project_state)
        {
            return Err(format!(
                "project source root `{}` failed interface preparation: {message}",
                root.display()
            ));
        }
    }

    Ok((Some(context), project_state))
}

/// Discovers the nearest Terlan project containing a test file.
///
/// Inputs:
/// - `test_path`: source path passed to `terlc test`.
/// - `explicit_cache_dir`: optional global cache directory.
///
/// Output:
/// - Project context when an ancestor `terlan.toml` exists.
/// - `None` for standalone test files.
///
/// Transformation:
/// - Canonicalizes the test path, searches ancestors for the project manifest,
///   resolves manifest source roots against the project root.
pub(super) fn discover_test_project_context(
    test_path: &Path,
    explicit_cache_dir: Option<&PathBuf>,
) -> Result<Option<TestProjectContext>, String> {
    let Some(root) = discover_test_project_root(test_path) else {
        return Ok(None);
    };

    let manifest_path = root.join("terlan.toml");
    let manifest = read_vm_test_project_manifest(&manifest_path)
        .map_err(|err| format!("failed to read test project manifest: {err}"))?;
    let cache_dir = explicit_cache_dir
        .cloned()
        .unwrap_or_else(|| root.join(".terlan"));
    let dependencies = crate::commands::build::resolve_project_test_dependencies(&root, &manifest)
        .map_err(|error| format!("failed to resolve test project dependencies: {error}"))?;

    let coverage_source_roots = manifest
        .source_roots
        .iter()
        .map(|source_root| root.join(source_root))
        .collect();
    Ok(Some(TestProjectContext {
        project_root: root,
        cache_dir,
        compilation_source_roots: dependencies.source_roots,
        coverage_source_roots,
        native_helper_environment: dependencies.native_helper_environment,
        native_helper_candidates: dependencies.native_helper_candidates,
    }))
}

/// Resolves only native helpers whose provider modules survived test closure pruning.
pub(super) fn reachable_native_helper_environment(
    context: Option<&TestProjectContext>,
    cores: &[crate::terlan_typeck::CoreModule],
) -> Result<Vec<(String, PathBuf)>, String> {
    let Some(context) = context else {
        return Ok(Vec::new());
    };
    let reachable_modules = cores
        .iter()
        .map(|core| core.module.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut environment = context.native_helper_environment.clone();
    environment.extend(crate::commands::build::build_reachable_test_native_helpers(
        &context.native_helper_candidates,
        &reachable_modules,
    )?);
    environment.sort_by(|left, right| left.0.cmp(&right.0));
    environment.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    Ok(environment)
}

/// Finds the nearest owning project without resolving or building dependencies.
pub(super) fn discover_test_project_root(test_path: &Path) -> Option<PathBuf> {
    let canonical_test = fs::canonicalize(test_path).ok()?;
    let mut current = canonical_test.parent();
    while let Some(dir) = current {
        if dir.join("terlan.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Reads project metadata for VM-owned test execution.
///
/// Inputs:
/// - `manifest_path`: path to the owning `terlan.toml`.
///
/// Output:
/// - Parsed project manifest for test source-root discovery.
/// - Error text from the normal project-manifest parser when the manifest is
///   invalid for test execution.
///
/// Transformation:
/// - Uses the normal parser and does not apply any legacy compatibility fallback.
pub(super) fn read_vm_test_project_manifest(
    manifest_path: &Path,
) -> Result<crate::commands::build::project_manifest::ProjectManifest, String> {
    crate::commands::build::project_manifest::read_project_manifest(manifest_path)
}

/// Collects test source files below a directory.
///
/// Inputs:
/// - `dir`: directory to traverse.
/// - `files`: accumulator for discovered test files.
///
/// Output:
/// - `Ok(())` when traversal succeeds.
/// - `Err(message)` when the directory cannot be read.
///
/// Transformation:
/// - Recursively walks the directory tree and records only files accepted by
///   the `*Test.terl` or legacy `*_test.terl` source layout predicate.
pub(super) fn collect_test_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("failed to read test directory {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read test directory entry in {}: {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_test_files(&path, files)?;
        } else if path.to_str().is_some_and(is_test_source_path) {
            files.push(path);
        }
    }
    Ok(())
}

/// Returns whether a source path is accepted by the test-file layout.
///
/// Inputs:
/// - `path`: user-provided source path passed to `terlc test`.
///
/// Output:
/// - `true` when the file stem ends in `Test` or `_test` and the extension is
///   `.terl`.
///
/// Transformation:
/// - Reads only the final path component and accepts both the canonical Terlan
///   test-module suffix and the legacy underscore suffix used by pre-0.0.7
///   app tests.
pub(super) fn is_test_source_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            Path::new(name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.ends_with("Test") || stem.ends_with("_test"))
                && name.ends_with(".terl")
        })
}

/// Builds target-profile options for test compilation paths.
///
/// Inputs:
/// - `state`: CLI state carrying native policy.
/// - `allow_asset_imports`: whether the test path owns asset import resolution.
///
/// Output:
/// - Target-profile validation options for primary and support test modules.
///
/// Transformation:
/// - Keeps test validation aligned with package build: NativeBoundary-backed std
///   APIs are admitted only when native policy is not pure.
pub(super) fn test_target_profile_options(
    state: &CliState,
    allow_asset_imports: bool,
) -> TargetProfileCheckOptions {
    TargetProfileCheckOptions {
        allow_asset_imports,
        allow_rust_backed_std_modules: state.native_policy != NativePolicy::Pure,
    }
}
