use super::*;

#[cfg(test)]
#[path = "test_dependencies_test.rs"]
mod test_dependencies_test;

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
    let roots = resolve_project_build_roots(project_dir, manifest)?;
    let source_roots = roots
        .source_roots
        .into_iter()
        .map(|root| root.path)
        .collect();
    let native_helper_environment = roots.native_artifact_environment;
    let artifact_helpers = native_helper_environment
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let native_helper_candidates = roots
        .native_rust_dependencies
        .iter()
        .filter(|dependency| !artifact_helpers.contains(dependency.native.helper_env.as_str()))
        .map(test_native_helper_candidate)
        .collect::<Vec<_>>();
    Ok(ResolvedProjectTestDependencies {
        source_roots,
        native_helper_environment,
        native_helper_candidates,
    })
}

/// Dependency context shared by project builds and in-process VM tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProjectTestDependencies {
    /// Dependency-first source roots selected by normal package resolution.
    pub(crate) source_roots: Vec<PathBuf>,
    /// Verified or freshly built native helper bindings for in-process VM tests.
    pub(crate) native_helper_environment: Vec<(String, PathBuf)>,
    /// Source-backed helpers deferred until focused import reachability is known.
    pub(crate) native_helper_candidates: Vec<TestNativeHelperCandidate>,
}

/// One source-backed native helper that may be reachable from a focused test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestNativeHelperCandidate {
    pub(crate) namespace: Option<String>,
    pub(crate) helper_env: String,
    helper: String,
    features: Vec<String>,
    crate_dir: PathBuf,
}

impl TestNativeHelperCandidate {
    /// Files that bind result-cache reuse to the native provider source.
    pub(crate) fn fingerprint_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let mut pending = vec![self.crate_dir.clone()];
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries.into_iter().rev() {
                let path = entry.path();
                if entry.file_name() == "target" || entry.file_name() == ".git" {
                    continue;
                }
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => pending.push(path),
                    Ok(kind) if kind.is_file() => paths.push(path),
                    _ => {}
                }
            }
        }
        paths.sort();
        paths
    }
}

fn test_native_helper_candidate(
    dependency: &ProjectNativeRustDependency,
) -> TestNativeHelperCandidate {
    TestNativeHelperCandidate {
        namespace: dependency.package.namespace.clone(),
        helper_env: dependency.native.helper_env.clone(),
        helper: dependency.native.helper.clone(),
        features: dependency.native.features.clone(),
        crate_dir: dependency.package_dir.join(&dependency.native.path),
    }
}

/// Builds only helpers whose provider namespace occurs in the compiled closure.
///
/// Existing source-backed binaries still pass through Cargo so its dependency
/// fingerprints refresh helpers after provider source changes. Cargo reuses an
/// unchanged helper without recompiling it.
pub(crate) fn build_reachable_test_native_helpers(
    dependencies: &[TestNativeHelperCandidate],
    reachable_modules: &std::collections::BTreeSet<String>,
) -> Result<Vec<(String, PathBuf)>, String> {
    let mut bindings = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        if dependency.namespace.as_ref().is_some_and(|namespace| {
            !reachable_modules.iter().any(|module| {
                module == namespace
                    || module
                        .strip_prefix(namespace)
                        .is_some_and(|suffix| suffix.starts_with('.'))
            })
        }) {
            continue;
        }
        if let Some(path) = std::env::var_os(&dependency.helper_env) {
            let path = PathBuf::from(path);
            if !path.is_file() {
                return Err(format!(
                    "error[native_helper_unavailable]: native helper environment `{}` points at a missing file: {}",
                    dependency.helper_env,
                    path.display()
                ));
            }
            bindings.push((dependency.helper_env.clone(), path));
            continue;
        }
        let manifest_path = dependency.crate_dir.join("Cargo.toml");
        if !manifest_path.is_file() {
            return Err(format!(
                "error[native_helper_unavailable]: native helper `{}` manifest is missing: {}",
                dependency.helper,
                manifest_path.display()
            ));
        }
        let target_dir = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| dependency.crate_dir.join("target"));
        let helper_path = target_dir.join("debug").join(&dependency.helper);
        #[cfg(windows)]
        let helper_path = helper_path.with_extension("exe");
        let mut command = Command::new("cargo");
        command
            .arg("build")
            .arg("--manifest-path")
            .arg(&manifest_path)
            .arg("--bin")
            .arg(&dependency.helper);
        if !dependency.features.is_empty() {
            command.arg("--features").arg(dependency.features.join(","));
        }
        let output = command.output().map_err(|error| {
            format!(
                "failed to build native helper `{}` for tests: {error}",
                dependency.helper
            )
        })?;
        if !output.status.success() {
            return Err(format!(
                "failed to build native helper `{}` for tests\nstdout:\n{}\nstderr:\n{}",
                dependency.helper,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        if !helper_path.is_file() {
            return Err(format!(
                "native helper `{}` was not found after Cargo build at {}",
                dependency.helper,
                helper_path.display()
            ));
        }
        bindings.push((dependency.helper_env.clone(), helper_path));
    }
    Ok(bindings)
}
