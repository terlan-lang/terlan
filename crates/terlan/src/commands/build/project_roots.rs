use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::metadata::{ProjectBuildRoots, ProjectNativeRustDependency, ProjectSourceRoot};
use super::package_layout::source_package_path;
use super::{project_manifest, project_manifest_path, TERLAN_PROJECT_MANIFEST_FILE};

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
pub(super) fn resolve_project_build_roots(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
) -> Result<ProjectBuildRoots, String> {
    reject_unsupported_external_dependencies(manifest)?;
    let mut resolver = LocalDependencyResolver::default();
    let root_dir = canonical_project_dir(project_dir)?;
    resolver.resolve_package(&root_dir, manifest, false)?;
    Ok(ProjectBuildRoots {
        source_roots: resolver.source_roots,
        native_rust_dependencies: resolver.native_rust_dependencies,
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
pub(super) fn reject_unsupported_external_dependencies(
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
    native_rust_dependencies: Vec<ProjectNativeRustDependency>,
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
        is_local_dependency: bool,
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
                self.resolve_package(&dependency_dir, &dependency_manifest, true)?;
            }
        }

        if is_local_dependency {
            if let Some(native) = &manifest.native_rust {
                self.native_rust_dependencies
                    .push(ProjectNativeRustDependency {
                        package: manifest.package.clone(),
                        package_dir: project_dir.to_path_buf(),
                        native: native.clone(),
                    });
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
