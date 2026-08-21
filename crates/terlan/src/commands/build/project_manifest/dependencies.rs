use super::*;

/// Validates a project-local script path.
pub(super) fn validate_script_path(
    script_path: &str,
    path: &Path,
    line_no: usize,
) -> Result<(), String> {
    if script_path.trim().is_empty() {
        return Err(format!(
            "{}:{}: [scripts] path cannot be empty",
            path.display(),
            line_no
        ));
    }
    if script_path != script_path.trim() {
        return Err(format!(
            "{}:{}: [scripts] path `{script_path}` cannot contain leading or trailing whitespace",
            path.display(),
            line_no
        ));
    }
    let candidate = Path::new(script_path);
    if candidate.is_absolute() {
        return Err(format!(
            "{}:{}: [scripts] path `{script_path}` must be package-relative",
            path.display(),
            line_no
        ));
    }
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(format!(
            "{}:{}: [scripts] path `{script_path}` cannot use current-directory or parent traversal",
            path.display(),
            line_no
        ));
    }
    if candidate.extension().and_then(|ext| ext.to_str()) != Some("terls") {
        return Err(format!(
            "{}:{}: [scripts] path `{script_path}` must point to a .terls file",
            path.display(),
            line_no
        ));
    }
    Ok(())
}

/// Parses a supported target dependency section.
///
/// Inputs:
/// - `section`: section name without surrounding brackets.
///
/// Output:
/// - Target namespace when the section has the supported
///   `target.<name>.dependencies` shape.
///
/// Transformation:
/// - Converts target dependency section names into typed target scopes.
pub(super) fn parse_target_dependency_section(section: &str) -> Option<ProjectTarget> {
    match section {
        "target.js.dependencies" => Some(ProjectTarget::Js),
        "target.rust.dependencies" => Some(ProjectTarget::Rust),
        _ => None,
    }
}

/// Splits one manifest assignment into key and value text.
///
/// Inputs:
/// - `line`: trimmed source line.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Key and value slices with surrounding whitespace removed.
///
/// Transformation:
/// - Requires one `=` delimiter and leaves value parsing to the caller.
pub(super) fn split_key_value<'a>(
    line: &'a str,
    path: &Path,
    line_no: usize,
) -> Result<(&'a str, &'a str), String> {
    let (key, value) = line.split_once('=').ok_or_else(|| {
        format!(
            "{}:{}: project manifest assignment requires `=`",
            path.display(),
            line_no
        )
    })?;
    let key = key.trim();
    if key.is_empty() {
        return Err(format!(
            "{}:{}: project manifest key cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok((key, value.trim()))
}

/// Parses a project artifact kind.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Supported project artifact kind.
///
/// Transformation:
/// - Parses the value as a manifest string and narrows it to the artifact kinds
///   admitted by the project package contract.
pub(super) fn parse_artifact_kind(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectArtifactKind, String> {
    let parsed = parse_string(value, path, line_no)?;
    match parsed.as_str() {
        "terlan-vm" => Ok(ProjectArtifactKind::TerlanVm),
        "library" => Ok(ProjectArtifactKind::Library),
        "wasm-core" => Ok(ProjectArtifactKind::WasmCore),
        "wasm-browser" => Ok(ProjectArtifactKind::WasmBrowser),
        "wasm-component" => Ok(ProjectArtifactKind::WasmComponent),
        "wasi-cli" => Ok(ProjectArtifactKind::WasiCli),
        "wasi-http" => Ok(ProjectArtifactKind::WasiHttp),
        "wasi-worker" => Ok(ProjectArtifactKind::WasiWorker),
        other => Err(format!(
            "{}:{}: unsupported [build] artifact `{}`; supported artifacts: terlan-vm, library, wasm-core, wasm-browser, wasm-component, wasi-cli, wasi-http, wasi-worker",
            path.display(),
            line_no,
            other
        )),
    }
}

/// Parses one project dependency manifest entry.
///
/// Inputs:
/// - `scope`: dependency scope from the current manifest section.
/// - `alias`: dependency alias from the manifest key.
/// - `value`: inline dependency table source text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Parsed dependency metadata.
///
/// Transformation:
/// - Parses one inline manifest table and narrows it to the dependency source
///   kind admitted for the current scope without fetching any dependency.
pub(super) fn parse_dependency_entry(
    scope: ProjectDependencyScope,
    alias: &str,
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependency, String> {
    validate_dependency_alias(alias, path, line_no)?;
    let fields = parse_inline_table(value, path, line_no)?;
    let source = parse_dependency_source(scope, &fields, path, line_no)?;
    Ok(ProjectDependency {
        alias: alias.to_string(),
        scope,
        source,
    })
}

/// Parses one dependency source from inline-table fields.
///
/// Inputs:
/// - `scope`: dependency scope from the current manifest section.
/// - `fields`: parsed inline-table fields.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Dependency source metadata.
///
/// Transformation:
/// - Enforces the scope/source pairing:
///   - `[dependencies]` accepts `{ path = "..." }` or
///     `{ git = "...", rev = "..." }`.
///   - `[target.js.dependencies]` accepts `{ npm = "...", version = "..." }`.
///   - `[target.rust.dependencies]` accepts `{ cargo = "...", version = "...",
///     features = ["..."] }`, with `features` optional.
pub(super) fn parse_dependency_source(
    scope: ProjectDependencyScope,
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    match scope {
        ProjectDependencyScope::Local => parse_local_dependency_source(fields, path, line_no),
        ProjectDependencyScope::Target(ProjectTarget::Js) => {
            parse_external_registry_dependency_source("npm", fields, path, line_no).map(
                |(package, version, integrity)| ProjectDependencySource::Npm {
                    package,
                    version,
                    integrity,
                },
            )
        }
        ProjectDependencyScope::Target(ProjectTarget::Rust) => {
            parse_cargo_dependency_source(fields, path, line_no)
        }
    }
}

/// Parses local Terlan dependency source fields.
///
/// Inputs:
/// - `fields`: parsed dependency inline-table fields.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Local path or Git dependency source.
///
/// Transformation:
/// - Requires either exactly one `path` field or exactly `git` plus `rev` so
///   portable package dependencies are explicit and reproducible.
pub(super) fn parse_local_dependency_source(
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    if fields.len() == 1 && fields.contains_key("path") {
        return parse_path_dependency_source(fields, path, line_no);
    }
    if fields.len() == 2 && fields.contains_key("git") && fields.contains_key("rev") {
        return parse_git_dependency_source(fields, path, line_no);
    }
    if fields.len() == 2 && fields.contains_key("registry") && fields.contains_key("version") {
        let (registry, version) =
            parse_registry_dependency_source("registry", fields, path, line_no)?;
        return Ok(ProjectDependencySource::Registry { registry, version });
    }
    Err(format!(
        "{}:{}: [dependencies] entries must use {{ path = \"...\" }}, {{ git = \"...\", rev = \"...\" }}, or {{ registry = \"...\", version = \"...\" }}",
        path.display(),
        line_no
    ))
}

/// Parses local path dependency source fields.
pub(super) fn parse_path_dependency_source(
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    let dependency_path = expect_inline_string_field(fields, "path", path, line_no)?;
    if dependency_path.trim().is_empty() {
        return Err(format!(
            "{}:{}: dependency path cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok(ProjectDependencySource::Path {
        path: dependency_path,
    })
}

/// Parses local Git dependency source fields.
pub(super) fn parse_git_dependency_source(
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    let url = expect_inline_string_field(fields, "git", path, line_no)?;
    let rev = expect_inline_string_field(fields, "rev", path, line_no)?;
    if url.trim().is_empty() {
        return Err(format!(
            "{}:{}: dependency git URL cannot be empty",
            path.display(),
            line_no
        ));
    }
    if rev.trim().is_empty() {
        return Err(format!(
            "{}:{}: dependency git rev cannot be empty",
            path.display(),
            line_no
        ));
    }
    if !matches!(rev.len(), 40 | 64) || !rev.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{}:{}: dependency git rev must be a full 40- or 64-character hexadecimal commit id, found `{rev}`",
            path.display(),
            line_no
        ));
    }
    Ok(ProjectDependencySource::Git { url, rev })
}

/// Parses target registry dependency source fields.
///
/// Inputs:
/// - `source_key`: expected registry field key, such as `hex`, `npm`, or
///   `cargo`.
/// - `fields`: parsed dependency inline-table fields.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Registry package name and version.
///
/// Transformation:
/// - Requires exactly the target source key and `version`; this preserves
///   metadata while preventing the generic manifest parser from accepting
///   target-package-manager options it cannot validate yet.
pub(super) fn parse_registry_dependency_source(
    source_key: &str,
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<(String, String), String> {
    if fields.len() != 2 || !fields.contains_key(source_key) || !fields.contains_key("version") {
        return Err(format!(
            "{}:{}: target dependency entries must use exactly {{ {} = \"...\", version = \"...\" }}",
            path.display(),
            line_no,
            source_key
        ));
    }
    let package = expect_inline_string_field(fields, source_key, path, line_no)?;
    let version = expect_inline_string_field(fields, "version", path, line_no)?;
    if package.trim().is_empty() {
        return Err(format!(
            "{}:{}: target dependency package name cannot be empty",
            path.display(),
            line_no
        ));
    }
    if version.trim().is_empty() {
        return Err(format!(
            "{}:{}: target dependency version cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok((package, version))
}

/// Parses an exact target-ecosystem dependency plus optional lock integrity.
pub(super) fn parse_external_registry_dependency_source(
    source_key: &str,
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<(String, String, Option<String>), String> {
    let has_required = fields.contains_key(source_key) && fields.contains_key("version");
    let has_only_allowed = fields
        .keys()
        .all(|key| matches!(key.as_str(), "npm" | "version" | "integrity"));
    if !has_required || !has_only_allowed {
        return Err(format!(
            "{}:{}: target dependency entries must use {{ {} = \"...\", version = \"...\" }} with optional integrity = \"sha256:<digest>\"",
            path.display(),
            line_no,
            source_key
        ));
    }
    let package = expect_inline_string_field(fields, source_key, path, line_no)?;
    let version = expect_inline_string_field(fields, "version", path, line_no)?;
    let integrity = fields
        .contains_key("integrity")
        .then(|| expect_inline_string_field(fields, "integrity", path, line_no))
        .transpose()?;
    if package.trim().is_empty() || version.trim().is_empty() {
        return Err(format!(
            "{}:{}: target dependency package and version cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok((package, version, integrity))
}

/// Parses Rust Cargo dependency source fields.
///
/// Inputs:
/// - `fields`: parsed dependency inline-table fields.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Cargo package name, version, and optional feature list.
///
/// Transformation:
/// - Accepts the same package/version fields as other registry dependencies,
///   plus an optional `features = ["..."]` list needed by native Rust package
///   probes such as Polars.
pub(super) fn parse_cargo_dependency_source(
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    let has_required = fields.contains_key("cargo") && fields.contains_key("version");
    let has_only_allowed = fields
        .keys()
        .all(|key| matches!(key.as_str(), "cargo" | "version" | "integrity" | "features"));
    if !has_required || !has_only_allowed {
        return Err(format!(
            "{}:{}: target rust dependency entries must use {{ cargo = \"...\", version = \"...\" }} with optional features = [\"...\"]",
            path.display(),
            line_no
        ));
    }
    let package = expect_inline_string_field(fields, "cargo", path, line_no)?;
    let version = expect_inline_string_field(fields, "version", path, line_no)?;
    let integrity = fields
        .contains_key("integrity")
        .then(|| expect_inline_string_field(fields, "integrity", path, line_no))
        .transpose()?;
    let features = if fields.contains_key("features") {
        expect_inline_string_array_field(fields, "features", path, line_no)?
    } else {
        Vec::new()
    };
    if package.trim().is_empty() {
        return Err(format!(
            "{}:{}: target dependency package name cannot be empty",
            path.display(),
            line_no
        ));
    }
    if version.trim().is_empty() {
        return Err(format!(
            "{}:{}: target dependency version cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok(ProjectDependencySource::Cargo {
        package,
        version,
        integrity,
        features,
    })
}
