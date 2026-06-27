use super::*;

/// Validates a dependency alias key.
///
/// Inputs:
/// - `alias`: dependency alias from the manifest key.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - `Ok(())` when the alias is accepted.
/// - `Err(String)` when the alias cannot be used as stable dependency
///   metadata.
///
/// Transformation:
/// - Uses the same package-root spelling subset as package names so dependency
///   aliases remain stable across target adapters.
pub(super) fn validate_dependency_alias(
    alias: &str,
    path: &Path,
    line_no: usize,
) -> Result<(), String> {
    let mut chars = alias.chars();
    let Some(first) = chars.next() else {
        return Err(format!(
            "{}:{}: project dependency alias cannot be empty",
            path.display(),
            line_no
        ));
    };
    if !first.is_ascii_lowercase() {
        return Err(format!(
            "{}:{}: project dependency alias must start with a lowercase ASCII letter",
            path.display(),
            line_no
        ));
    }
    if chars.any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')) {
        return Err(format!(
            "{}:{}: project dependency alias may contain only lowercase ASCII letters, digits, `_`, or `-`",
            path.display(),
            line_no
        ));
    }
    Ok(())
}

/// Validates the package name accepted by the project manifest.
///
/// Inputs:
/// - `name`: parsed package name.
/// - `path`: manifest path used in diagnostics.
///
/// Output:
/// - `Ok(())` when the name is accepted.
/// - `Err(String)` when the name cannot be used as a package root.
///
/// Transformation:
/// - Enforces the package-root naming subset used by module layout validation:
///   lower-case ASCII start, followed by lower-case ASCII letters, digits,
///   `_`, or `-`.
pub(super) fn validate_package_name(name: &str, path: &Path) -> Result<(), String> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(format!(
            "{}: project manifest [package] name cannot be empty",
            path.display()
        ));
    };
    if !first.is_ascii_lowercase() {
        return Err(format!(
            "{}: project manifest [package] name must start with a lowercase ASCII letter",
            path.display()
        ));
    }
    if chars.any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')) {
        return Err(format!(
            "{}: project manifest [package] name may contain only lowercase ASCII letters, digits, `_`, or `-`",
            path.display()
        ));
    }
    Ok(())
}

/// Validates an optional package namespace.
///
/// Inputs:
/// - `namespace`: parsed `[package] namespace` value.
/// - `path`: manifest path used in diagnostics.
///
/// Output:
/// - `Ok(())` when the namespace is a dot-separated lowercase module prefix.
/// - `Err(String)` when the namespace cannot be mapped onto source layout.
///
/// Transformation:
/// - Keeps first-party package namespace grants explicit without weakening
///   package names. Every namespace segment must be a source-path-compatible
///   lower-case module segment.
pub(super) fn validate_package_namespace(namespace: &str, path: &Path) -> Result<(), String> {
    if namespace.trim().is_empty() {
        return Err(format!(
            "{}: project manifest [package] namespace cannot be empty",
            path.display()
        ));
    }
    for segment in namespace.split('.') {
        validate_package_namespace_segment(segment, namespace, path)?;
    }
    Ok(())
}

/// Validates one package namespace segment.
///
/// Inputs:
/// - `segment`: one dot-separated namespace segment.
/// - `namespace`: full namespace used in diagnostics.
/// - `path`: manifest path used in diagnostics.
///
/// Output:
/// - `Ok(())` when the segment is accepted.
/// - `Err(String)` when the segment is empty or contains unsupported
///   characters.
///
/// Transformation:
/// - Applies the same lowercase source-path character policy as package roots,
///   but disallows `-` because namespace segments are Terlan module segments.
fn validate_package_namespace_segment(
    segment: &str,
    namespace: &str,
    path: &Path,
) -> Result<(), String> {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return Err(format!(
            "{}: project manifest [package] namespace `{}` contains an empty segment",
            path.display(),
            namespace
        ));
    };
    if !first.is_ascii_lowercase() {
        return Err(format!(
            "{}: project manifest [package] namespace `{}` segments must start with a lowercase ASCII letter",
            path.display(),
            namespace
        ));
    }
    if chars.any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')) {
        return Err(format!(
            "{}: project manifest [package] namespace `{}` segments may contain only lowercase ASCII letters, digits, or `_`",
            path.display(),
            namespace
        ));
    }
    Ok(())
}

/// Validates the package version accepted by the project manifest.
///
/// Inputs:
/// - `version`: parsed package version.
/// - `path`: manifest path used in diagnostics.
///
/// Output:
/// - `Ok(())` when the version is accepted.
/// - `Err(String)` when the version cannot identify a package build.
///
/// Transformation:
/// - Enforces a small SemVer-like numeric core (`major.minor.patch`) and allows
///   optional pre-release/build suffix characters without interpreting them.
pub(super) fn validate_package_version(version: &str, path: &Path) -> Result<(), String> {
    let core = version
        .split(['-', '+'])
        .next()
        .expect("split always returns at least one item");
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(format!(
            "{}: project manifest [package] version must use major.minor.patch form",
            path.display()
        ));
    }
    if !parts
        .iter()
        .all(|part| part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return Err(format!(
            "{}: project manifest [package] version numeric core must contain only digits",
            path.display()
        ));
    }
    if version
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '+' || ch == '_'))
    {
        return Err(format!(
            "{}: project manifest [package] version contains unsupported characters",
            path.display()
        ));
    }
    Ok(())
}
