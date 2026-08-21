use super::*;

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
    let mut native_helper_environment = roots.native_artifact_environment;
    let artifact_helpers = native_helper_environment
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let source_dependencies = roots
        .native_rust_dependencies
        .iter()
        .filter(|dependency| !artifact_helpers.contains(dependency.native.helper_env.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    native_helper_environment.extend(build_test_native_helpers(&source_dependencies)?);
    Ok(ResolvedProjectTestDependencies {
        source_roots,
        native_helper_environment,
    })
}

/// Dependency context shared by project builds and in-process VM tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProjectTestDependencies {
    /// Dependency-first source roots selected by normal package resolution.
    pub(crate) source_roots: Vec<PathBuf>,
    /// Verified or freshly built native helper bindings for in-process VM tests.
    pub(crate) native_helper_environment: Vec<(String, PathBuf)>,
}

fn build_test_native_helpers(
    dependencies: &[ProjectNativeRustDependency],
) -> Result<Vec<(String, PathBuf)>, String> {
    let mut bindings = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        let native = &dependency.native;
        if let Some(path) = std::env::var_os(&native.helper_env) {
            let path = PathBuf::from(path);
            if !path.is_file() {
                return Err(format!(
                    "error[native_helper_unavailable]: native helper environment `{}` points at a missing file: {}",
                    native.helper_env,
                    path.display()
                ));
            }
            bindings.push((native.helper_env.clone(), path));
            continue;
        }
        let crate_dir = dependency.package_dir.join(&native.path);
        let manifest_path = crate_dir.join("Cargo.toml");
        if !manifest_path.is_file() {
            return Err(format!(
                "error[native_helper_unavailable]: native helper `{}` manifest is missing: {}",
                native.helper,
                manifest_path.display()
            ));
        }
        let target_dir = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate_dir.join("target"));
        let helper_path = target_dir.join("debug").join(&native.helper);
        #[cfg(windows)]
        let helper_path = helper_path.with_extension("exe");
        if !helper_path.is_file() || !native.features.is_empty() {
            let mut command = Command::new("cargo");
            command
                .arg("build")
                .arg("--manifest-path")
                .arg(&manifest_path)
                .arg("--bin")
                .arg(&native.helper);
            if !native.features.is_empty() {
                command.arg("--features").arg(native.features.join(","));
            }
            let output = command.output().map_err(|error| {
                format!(
                    "failed to build native helper `{}` for tests: {error}",
                    native.helper
                )
            })?;
            if !output.status.success() {
                return Err(format!(
                    "failed to build native helper `{}` for tests\nstdout:\n{}\nstderr:\n{}",
                    native.helper,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
        if !helper_path.is_file() {
            return Err(format!(
                "native helper `{}` was not found after Cargo build at {}",
                native.helper,
                helper_path.display()
            ));
        }
        bindings.push((native.helper_env.clone(), helper_path));
    }
    Ok(bindings)
}
