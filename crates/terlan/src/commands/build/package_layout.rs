use std::path::Path;

use super::project_manifest;

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
pub(super) fn source_package_path(package: &project_manifest::ProjectPackage) -> Vec<String> {
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
pub(super) fn source_package_module_prefix(package: &project_manifest::ProjectPackage) -> String {
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
pub(super) fn validate_project_source_package_root(
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
