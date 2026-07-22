use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::artifacts::{fingerprint, read_manifest};
use crate::commands::source_layout::{
    expected_module_name_for_source_path, validate_module_layout,
};
use crate::terlan_hir::{parse_interface_file, syntax_module_output_to_interface};
use crate::terlan_syntax::syntax_contract_identity_matches_current;
use crate::terlan_typeck::expand_syntax_raw_macros;
use crate::CliState;

use super::{write_build_file, TERLAN_PROJECT_MANIFEST_FILE};

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
        if state.incremental && cached_interface_is_current(root, &file, &source, cache_dir) {
            continue;
        }
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
        validate_module_layout(root, &file, &syntax_output.module_name)?;
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

/// Determines whether an implementation's emitted interface can skip parsing.
///
/// Inputs:
/// - `root`: source root that defines path-derived module identity.
/// - `file`: implementation source considered by the interface prepass.
/// - `source`: current implementation source text.
/// - `cache_dir`: compiler-private interface cache directory.
///
/// Output:
/// - `true` only for a complete, current `.typi` plus `.typi.deps` pair.
///
/// Transformation:
/// - Derives module identity without parsing implementation syntax, validates
///   the source and syntax-contract identities from the dependency manifest,
///   parses the signature-only interface artifact, and verifies both its type
///   and documentation hashes. Checked executable implementation reuse applies
///   the stronger current-dependency validation separately.
pub(super) fn cached_interface_is_current(
    root: &Path,
    file: &Path,
    source: &str,
    cache_dir: &Path,
) -> bool {
    let Ok(module) = expected_module_name_for_source_path(root, file) else {
        return false;
    };
    let Some(manifest) = read_manifest(&cache_dir.join(format!("{module}.typi.deps"))) else {
        return false;
    };
    if manifest.module != module
        || manifest.source_hash != fingerprint(source.as_bytes())
        || !syntax_contract_identity_matches_current(&manifest.syntax_contract_identity)
            .unwrap_or(false)
    {
        return false;
    }
    let Some((interface_module, interface)) =
        parse_interface_file(&cache_dir.join(format!("{module}.typi")))
    else {
        return false;
    };
    interface_module == module
        && manifest.interface_hash
            == fingerprint(interface.to_terlan_interface_type_text().as_bytes())
        && manifest.interface_doc_hash
            == fingerprint(interface.to_terlan_interface_doc_text().as_bytes())
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
pub(super) fn report_empty_source_root(root: &Path) {
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
