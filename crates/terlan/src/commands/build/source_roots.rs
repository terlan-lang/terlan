use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::source_layout::expected_module_name_for_source_path;
use crate::terlan_hir::syntax_module_output_to_interface;
use crate::terlan_typeck::expand_syntax_raw_macros;
use crate::CliState;

use super::{
    build_one_erlang_source_artifact, validate_build_entrypoint,
    validate_project_source_package_root, write_build_debug_map, write_build_executable_launcher,
    write_build_file, write_build_package_metadata, BuildDebugProject, BuildModuleArtifact,
    BuildOneError, BuildPackageMetadata, BuildTimings, ProjectSourceRoot,
    TERLAN_PROJECT_MANIFEST_FILE,
};

pub(super) fn run_erlang_source_root_build(dir: &Path, state: &CliState) -> ExitCode {
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
pub(super) fn run_erlang_project_source_roots_build(
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
pub(super) struct SourceRootBuildUnit {
    pub(super) path: PathBuf,
    pub(super) package_path: Option<Vec<String>>,
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
