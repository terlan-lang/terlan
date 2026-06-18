use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Parsed Terlan project manifest.
///
/// Inputs:
/// - Produced from `terlan.toml`.
///
/// Output:
/// - Build-owned project metadata used by future project-directory builds.
///
/// Transformation:
/// - Stores the package identity, declared source roots, and requested artifact
///   kind after project package validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectManifest {
    pub(crate) package: ProjectPackage,
    pub(crate) source_roots: Vec<String>,
    pub(crate) artifact: ProjectArtifactKind,
    pub(crate) web_assets: Option<ProjectWebAssets>,
    pub(crate) dependencies: Vec<ProjectDependency>,
    pub(crate) erlang_package_adapter: Option<ProjectErlangPackageAdapter>,
}

/// Parsed package metadata from `[package]`.
///
/// Inputs:
/// - Produced from the manifest `[package]` table.
///
/// Output:
/// - Stable package identity for project build diagnostics and future artifact
///   metadata.
///
/// Transformation:
/// - Keeps only fields admitted into the A0.42.1 project package boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectPackage {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) namespace: Option<String>,
}

/// Parsed executable artifact kind from `[build]`.
///
/// Inputs:
/// - Produced from the manifest `[build] artifact` value or its default.
///
/// Output:
/// - Stable artifact kind for target packaging decisions.
///
/// Transformation:
/// - Narrows manifest text to the package artifact modes admitted by the
///   current 0.0.1 package contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectArtifactKind {
    BeamThin,
    Library,
}

impl ProjectArtifactKind {
    /// Returns the manifest spelling for the artifact kind.
    ///
    /// Inputs:
    /// - `self`: parsed artifact kind.
    ///
    /// Output:
    /// - Static manifest spelling.
    ///
    /// Transformation:
    /// - Converts the enum to the package-contract string used in diagnostics
    ///   and build metadata.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProjectArtifactKind::BeamThin => "beam-thin",
            ProjectArtifactKind::Library => "library",
        }
    }
}

/// Parsed Terlan-owned web asset configuration from `[web.assets]`.
///
/// Inputs:
/// - Produced from user-authored `terlan.toml`.
///
/// Output:
/// - Stable web asset configuration for browser packaging.
///
/// Transformation:
/// - Keeps the user-facing asset shape independent from Rsbuild/Rspack config
///   while preserving enough metadata for later packaging translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectWebAssets {
    pub(crate) directory: String,
    pub(crate) public_path: Option<String>,
    pub(crate) inline_limit: Option<u64>,
}

/// Parsed Erlang target packaging adapter reservation.
///
/// Inputs:
/// - Produced from `[target.erlang.package] adapter`.
///
/// Output:
/// - Stable adapter marker for downstream Erlang packaging metadata.
///
/// Transformation:
/// - Narrows manifest text to the adapter metadata admitted by A0.42.6 without
///   generating Rebar3 files or requiring Rebar3 during normal builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectErlangPackageAdapter {
    Rebar3Compatible,
}

impl ProjectErlangPackageAdapter {
    /// Returns the manifest spelling for the Erlang package adapter.
    ///
    /// Inputs:
    /// - `self`: parsed Erlang package adapter.
    ///
    /// Output:
    /// - Static manifest spelling.
    ///
    /// Transformation:
    /// - Converts the enum to the package metadata string used by build
    ///   artifacts and diagnostics.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProjectErlangPackageAdapter::Rebar3Compatible => "rebar3-compatible",
        }
    }
}

/// Parsed project dependency metadata.
///
/// Inputs:
/// - Produced from `[dependencies]` or `[target.<name>.dependencies]` manifest
///   entries.
///
/// Output:
/// - Typed dependency metadata for future dependency-closure and target-adapter
///   validation.
///
/// Transformation:
/// - Keeps the manifest alias, dependency scope, and source kind without
///   fetching, resolving, linking, or packaging the dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectDependency {
    pub(crate) alias: String,
    pub(crate) scope: ProjectDependencyScope,
    pub(crate) source: ProjectDependencySource,
}

/// Scope where a project dependency applies.
///
/// Inputs:
/// - Produced from the manifest dependency section name.
///
/// Output:
/// - Portable/local dependency scope or target-specific dependency scope.
///
/// Transformation:
/// - Separates local Terlan package dependencies from target ecosystem
///   dependency metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectDependencyScope {
    Local,
    Target(ProjectTarget),
}

/// Target namespace for target-scoped dependency metadata.
///
/// Inputs:
/// - Produced from `[target.<name>.dependencies]` section names.
///
/// Output:
/// - Known target namespace.
///
/// Transformation:
/// - Narrows manifest target names to package metadata scopes currently
///   recognized by the compiler roadmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectTarget {
    Erlang,
    Js,
    Rust,
}

/// Parsed dependency source kind.
///
/// Inputs:
/// - Produced from one inline dependency table.
///
/// Output:
/// - Source-specific dependency metadata.
///
/// Transformation:
/// - Preserves `path`, `hex`, `npm`, and `cargo` dependency source kinds
///   without performing dependency resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectDependencySource {
    Path {
        path: String,
    },
    Hex {
        package: String,
        version: String,
    },
    Npm {
        package: String,
        version: String,
    },
    Cargo {
        package: String,
        version: String,
        features: Vec<String>,
    },
}

/// Parsed dependency inline-table field value.
///
/// Inputs:
/// - Produced from one manifest inline dependency table.
///
/// Output:
/// - A string field or string-array field admitted by the manifest subset.
///
/// Transformation:
/// - Keeps dependency field parsing typed enough for Rust feature lists without
///   expanding the full TOML grammar into the hand-written manifest reader.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ManifestInlineValue {
    String(String),
    StringArray(Vec<String>),
}

/// Reads and parses a Terlan project manifest file.
///
/// Inputs:
/// - `path`: filesystem path to `terlan.toml`.
///
/// Output:
/// - `Ok(ProjectManifest)` when the file matches the A0.42.2 package and
///   dependency metadata shape.
/// - `Err(String)` when the file cannot be read or has unsupported manifest
///   syntax.
///
/// Transformation:
/// - Reads UTF-8 text from disk, then delegates to the manifest parser with the
///   path included in diagnostics.
pub(crate) fn read_project_manifest(path: &Path) -> Result<ProjectManifest, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("cannot read project manifest {}: {err}", path.display()))?;
    parse_project_manifest(&source, path)
}

/// Parses the A0.42.2 Terlan project package manifest shape.
///
/// Inputs:
/// - `source`: manifest text.
/// - `path`: manifest path used in diagnostics.
///
/// Output:
/// - `Ok(ProjectManifest)` with package identity, source roots, artifact kind,
///   and dependency metadata.
/// - `Err(String)` for missing package name, unsupported sections, unsupported
///   keys, malformed strings, malformed arrays, invalid artifact kinds, or
///   malformed dependency metadata.
///
/// Transformation:
/// - Applies a deliberately small TOML-like parser for the reviewed package
///   contract:
///   - `[package] name = "demo"` and `version = "0.0.1"`
///   - optional `[package] namespace = "std.native.polars"`
///   - optional `[build] source_roots = ["src", "lib"]`
///   - optional `[build] artifact = "beam-thin"`
///   - optional `[web.assets] directory = "assets"`
///   - optional `[web.assets] public_path = "/assets"`
///   - optional `[web.assets] inline_limit = 8192`
///   - `[dependencies] name = { path = "../name" }`
///   - `[target.erlang.dependencies] cowboy = { hex = "cowboy", version = "2.12.0" }`
///   - `[target.js.dependencies] zod = { npm = "zod", version = "3.25.0" }`
///   - `[target.rust.dependencies] serde = { cargo = "serde", version = "1.0.0" }`
///   - optional Rust feature flags:
///     `{ cargo = "polars", version = "0.54.4", features = ["lazy", "csv"] }`
/// - Defaults source roots to `["src"]` and artifact to `beam-thin` when
///   `[build]` omits them.
pub(crate) fn parse_project_manifest(source: &str, path: &Path) -> Result<ProjectManifest, String> {
    let mut section = ManifestSection::Root;
    let mut package_name = None;
    let mut package_version = None;
    let mut package_namespace = None;
    let mut source_roots = None;
    let mut artifact = None;
    let mut web_assets = ProjectWebAssetsBuilder::default();
    let mut dependencies = Vec::new();
    let mut erlang_package_adapter = None;

    for (index, raw_line) in source.lines().enumerate() {
        let line_no = index + 1;
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            section = parse_section(line, path, line_no)?;
            continue;
        }

        let (key, value) = split_key_value(line, path, line_no)?;
        match section {
            ManifestSection::Root => {
                return Err(format!(
                    "{}:{}: manifest keys must appear inside a supported project manifest section",
                    path.display(),
                    line_no
                ));
            }
            ManifestSection::Package => match key {
                "name" => {
                    package_name = Some(parse_string(value, path, line_no)?);
                }
                "version" => {
                    package_version = Some(parse_string(value, path, line_no)?);
                }
                "namespace" => {
                    package_namespace = Some(parse_string(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [package] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ManifestSection::Build => match key {
                "source_roots" => {
                    source_roots = Some(parse_string_array(value, path, line_no)?);
                }
                "artifact" => {
                    artifact = Some(parse_artifact_kind(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [build] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ManifestSection::WebAssets => match key {
                "directory" => {
                    web_assets.directory = Some(parse_string(value, path, line_no)?);
                }
                "public_path" => {
                    web_assets.public_path = Some(parse_string(value, path, line_no)?);
                }
                "inline_limit" => {
                    web_assets.inline_limit = Some(parse_non_negative_u64(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [web.assets] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ManifestSection::Dependencies => {
                dependencies.push(parse_dependency_entry(
                    ProjectDependencyScope::Local,
                    key,
                    value,
                    path,
                    line_no,
                )?);
            }
            ManifestSection::TargetDependencies(target) => {
                dependencies.push(parse_dependency_entry(
                    ProjectDependencyScope::Target(target),
                    key,
                    value,
                    path,
                    line_no,
                )?);
            }
            ManifestSection::TargetErlangPackage => match key {
                "adapter" => {
                    let adapter = parse_erlang_package_adapter(value, path, line_no)?;
                    if erlang_package_adapter.replace(adapter).is_some() {
                        return Err(format!(
                            "{}:{}: duplicate [target.erlang.package] adapter",
                            path.display(),
                            line_no
                        ));
                    }
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [target.erlang.package] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
        }
    }

    let name = package_name.ok_or_else(|| {
        format!(
            "{}: project manifest requires [package] name",
            path.display()
        )
    })?;
    if name.trim().is_empty() {
        return Err(format!(
            "{}: project manifest [package] name cannot be empty",
            path.display()
        ));
    }
    validate_package_name(&name, path)?;

    let version = package_version.ok_or_else(|| {
        format!(
            "{}: project manifest requires [package] version",
            path.display()
        )
    })?;
    if version.trim().is_empty() {
        return Err(format!(
            "{}: project manifest [package] version cannot be empty",
            path.display()
        ));
    }
    validate_package_version(&version, path)?;
    if let Some(namespace) = package_namespace.as_deref() {
        validate_package_namespace(namespace, path)?;
    }

    let source_roots = source_roots.unwrap_or_else(|| vec!["src".to_string()]);
    if source_roots.iter().any(|root| root.trim().is_empty()) {
        return Err(format!(
            "{}: project manifest [build] source_roots cannot contain empty entries",
            path.display()
        ));
    }
    let artifact = artifact.unwrap_or(ProjectArtifactKind::BeamThin);
    let web_assets = web_assets.finish(path)?;

    Ok(ProjectManifest {
        package: ProjectPackage {
            name,
            version,
            namespace: package_namespace,
        },
        source_roots,
        artifact,
        web_assets,
        dependencies,
        erlang_package_adapter,
    })
}

/// Incremental parser state for optional `[web.assets]`.
///
/// Inputs:
/// - Filled while scanning manifest key/value assignments.
///
/// Output:
/// - Optional `ProjectWebAssets` after validation.
///
/// Transformation:
/// - Distinguishes an absent section from a present but incomplete section so
///   users get a precise diagnostic when they start configuring web assets.
#[derive(Debug, Default)]
struct ProjectWebAssetsBuilder {
    directory: Option<String>,
    public_path: Option<String>,
    inline_limit: Option<u64>,
}

impl ProjectWebAssetsBuilder {
    /// Finalizes parsed web asset configuration.
    ///
    /// Inputs:
    /// - `self`: accumulated optional section values.
    /// - `path`: manifest path used in diagnostics.
    ///
    /// Output:
    /// - `Ok(None)` when `[web.assets]` was absent.
    /// - `Ok(Some(ProjectWebAssets))` when the section is complete.
    /// - `Err(String)` when the section is incomplete or invalid.
    ///
    /// Transformation:
    /// - Requires `directory` when any web asset key is present and rejects
    ///   empty path-like values before browser packaging consumes them.
    fn finish(self, path: &Path) -> Result<Option<ProjectWebAssets>, String> {
        let has_any_key =
            self.directory.is_some() || self.public_path.is_some() || self.inline_limit.is_some();
        if !has_any_key {
            return Ok(None);
        }
        let directory = self.directory.ok_or_else(|| {
            format!(
                "{}: project manifest [web.assets] requires directory",
                path.display()
            )
        })?;
        if directory.trim().is_empty() {
            return Err(format!(
                "{}: project manifest [web.assets] directory cannot be empty",
                path.display()
            ));
        }
        if let Some(public_path) = self.public_path.as_deref() {
            if public_path.trim().is_empty() {
                return Err(format!(
                    "{}: project manifest [web.assets] public_path cannot be empty",
                    path.display()
                ));
            }
        }
        Ok(Some(ProjectWebAssets {
            directory,
            public_path: self.public_path,
            inline_limit: self.inline_limit,
        }))
    }
}

/// Supported top-level manifest sections.
///
/// Inputs:
/// - Produced while scanning manifest text.
///
/// Output:
/// - Parser state controlling which keys are accepted.
///
/// Transformation:
/// - Narrows free-form section headers to the A0.42.1 manifest subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestSection {
    Root,
    Package,
    Build,
    WebAssets,
    Dependencies,
    TargetDependencies(ProjectTarget),
    TargetErlangPackage,
}

/// Removes unquoted line comments from one manifest line.
///
/// Inputs:
/// - `line`: one raw source line.
///
/// Output:
/// - Slice before the first unquoted `#`, or the whole line when there is no
///   comment.
///
/// Transformation:
/// - Scans quote and escape state so `#` inside strings is preserved.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

/// Parses a manifest section header.
///
/// Inputs:
/// - `line`: trimmed source line beginning with `[`.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Supported `ManifestSection`.
///
/// Transformation:
/// - Accepts exact `[package]`, `[build]`, `[dependencies]`,
///   `[target.<name>.dependencies]`, and `[target.erlang.package]` section
///   headers.
fn parse_section(line: &str, path: &Path, line_no: usize) -> Result<ManifestSection, String> {
    let section = line
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .ok_or_else(|| {
            format!(
                "{}:{}: malformed project manifest section",
                path.display(),
                line_no
            )
        })?;
    match section.trim() {
        "package" => Ok(ManifestSection::Package),
        "build" => Ok(ManifestSection::Build),
        "web.assets" => Ok(ManifestSection::WebAssets),
        "dependencies" => Ok(ManifestSection::Dependencies),
        "target.erlang.package" => Ok(ManifestSection::TargetErlangPackage),
        other => {
            if let Some(target) = parse_target_dependency_section(other) {
                Ok(ManifestSection::TargetDependencies(target))
            } else {
                Err(format!(
                    "{}:{}: unsupported project manifest section `{}`",
                    path.display(),
                    line_no,
                    other
                ))
            }
        }
    }
}

/// Parses an Erlang package adapter reservation.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Supported Erlang package adapter marker.
///
/// Transformation:
/// - Parses a manifest string and admits only the Rebar3-compatible adapter
///   reservation, without enabling Rebar3 file generation or invocation.
fn parse_erlang_package_adapter(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectErlangPackageAdapter, String> {
    let parsed = parse_string(value, path, line_no)?;
    match parsed.as_str() {
        "rebar3-compatible" => Ok(ProjectErlangPackageAdapter::Rebar3Compatible),
        other => Err(format!(
            "{}:{}: unsupported [target.erlang.package] adapter `{}`; supported adapters: rebar3-compatible",
            path.display(),
            line_no,
            other
        )),
    }
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
fn parse_target_dependency_section(section: &str) -> Option<ProjectTarget> {
    match section {
        "target.erlang.dependencies" => Some(ProjectTarget::Erlang),
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
fn split_key_value<'a>(
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
fn parse_artifact_kind(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectArtifactKind, String> {
    let parsed = parse_string(value, path, line_no)?;
    match parsed.as_str() {
        "beam-thin" => Ok(ProjectArtifactKind::BeamThin),
        "library" => Ok(ProjectArtifactKind::Library),
        other => Err(format!(
            "{}:{}: unsupported [build] artifact `{}`; supported artifacts: beam-thin, library",
            path.display(),
            line_no,
            other
        )),
    }
}

/// Parses a non-negative unsigned integer manifest value.
///
/// Inputs:
/// - `value`: trimmed manifest value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Parsed `u64` value.
///
/// Transformation:
/// - Accepts plain ASCII decimal digits only so user-authored TOML config stays
///   predictable and does not inherit target-tool numeric syntax variants.
fn parse_non_negative_u64(value: &str, path: &Path, line_no: usize) -> Result<u64, String> {
    if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(format!(
            "{}:{}: project manifest value must be a non-negative integer",
            path.display(),
            line_no
        ));
    }
    value.parse::<u64>().map_err(|err| {
        format!(
            "{}:{}: project manifest integer value is out of range: {err}",
            path.display(),
            line_no
        )
    })
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
fn parse_dependency_entry(
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
///   - `[dependencies]` accepts only `{ path = "..." }`.
///   - `[target.erlang.dependencies]` accepts `{ hex = "...", version = "..." }`.
///   - `[target.js.dependencies]` accepts `{ npm = "...", version = "..." }`.
///   - `[target.rust.dependencies]` accepts `{ cargo = "...", version = "...",
///     features = ["..."] }`, with `features` optional.
fn parse_dependency_source(
    scope: ProjectDependencyScope,
    fields: &BTreeMap<String, ManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    match scope {
        ProjectDependencyScope::Local => parse_path_dependency_source(fields, path, line_no),
        ProjectDependencyScope::Target(ProjectTarget::Erlang) => {
            parse_registry_dependency_source("hex", fields, path, line_no)
                .map(|(package, version)| ProjectDependencySource::Hex { package, version })
        }
        ProjectDependencyScope::Target(ProjectTarget::Js) => {
            parse_registry_dependency_source("npm", fields, path, line_no)
                .map(|(package, version)| ProjectDependencySource::Npm { package, version })
        }
        ProjectDependencyScope::Target(ProjectTarget::Rust) => {
            parse_cargo_dependency_source(fields, path, line_no)
        }
    }
}

/// Parses local path dependency source fields.
///
/// Inputs:
/// - `fields`: parsed dependency inline-table fields.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Local path dependency source.
///
/// Transformation:
/// - Requires exactly one `path` field and rejects version/source registry
///   metadata in portable local dependency sections.
fn parse_path_dependency_source(
    fields: &BTreeMap<String, ManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    if fields.len() != 1 || !fields.contains_key("path") {
        return Err(format!(
            "{}:{}: [dependencies] entries must use exactly {{ path = \"...\" }}",
            path.display(),
            line_no
        ));
    }
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
fn parse_registry_dependency_source(
    source_key: &str,
    fields: &BTreeMap<String, ManifestInlineValue>,
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
fn parse_cargo_dependency_source(
    fields: &BTreeMap<String, ManifestInlineValue>,
    path: &Path,
    line_no: usize,
) -> Result<ProjectDependencySource, String> {
    let has_required = fields.contains_key("cargo") && fields.contains_key("version");
    let has_only_allowed = fields
        .keys()
        .all(|key| matches!(key.as_str(), "cargo" | "version" | "features"));
    if !has_required || !has_only_allowed {
        return Err(format!(
            "{}:{}: target rust dependency entries must use {{ cargo = \"...\", version = \"...\" }} with optional features = [\"...\"]",
            path.display(),
            line_no
        ));
    }
    let package = expect_inline_string_field(fields, "cargo", path, line_no)?;
    let version = expect_inline_string_field(fields, "version", path, line_no)?;
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
        features,
    })
}

/// Returns a dependency inline-table string field.
///
/// Inputs:
/// - `fields`: parsed inline-table fields.
/// - `key`: field name expected to contain a string.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Field string value.
///
/// Transformation:
/// - Rejects string-array values where the dependency contract requires a
///   scalar string.
fn expect_inline_string_field(
    fields: &BTreeMap<String, ManifestInlineValue>,
    key: &str,
    path: &Path,
    line_no: usize,
) -> Result<String, String> {
    match fields.get(key) {
        Some(ManifestInlineValue::String(value)) => Ok(value.clone()),
        Some(ManifestInlineValue::StringArray(_)) => Err(format!(
            "{}:{}: project dependency field `{}` must be a string",
            path.display(),
            line_no,
            key
        )),
        None => Err(format!(
            "{}:{}: project dependency field `{}` is missing",
            path.display(),
            line_no,
            key
        )),
    }
}

/// Returns a dependency inline-table string-array field.
///
/// Inputs:
/// - `fields`: parsed inline-table fields.
/// - `key`: field name expected to contain an array of strings.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Field string-array value.
///
/// Transformation:
/// - Rejects scalar values where the dependency contract requires a list.
fn expect_inline_string_array_field(
    fields: &BTreeMap<String, ManifestInlineValue>,
    key: &str,
    path: &Path,
    line_no: usize,
) -> Result<Vec<String>, String> {
    match fields.get(key) {
        Some(ManifestInlineValue::StringArray(value)) => Ok(value.clone()),
        Some(ManifestInlineValue::String(_)) => Err(format!(
            "{}:{}: project dependency field `{}` must be an array of strings",
            path.display(),
            line_no,
            key
        )),
        None => Err(format!(
            "{}:{}: project dependency field `{}` is missing",
            path.display(),
            line_no,
            key
        )),
    }
}

/// Parses a one-line manifest inline table.
///
/// Inputs:
/// - `value`: trimmed inline-table source text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Ordered map of typed fields.
///
/// Transformation:
/// - Parses the reviewed `{ key = "value", features = ["..."] }` subset and
///   rejects duplicate keys, empty fields, and unsupported values.
fn parse_inline_table(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<BTreeMap<String, ManifestInlineValue>, String> {
    let inner = value
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .ok_or_else(|| {
            format!(
                "{}:{}: project dependency value must be an inline table",
                path.display(),
                line_no
            )
        })?;
    let mut fields = BTreeMap::new();
    for item in split_array_items(inner, path, line_no)? {
        let (key, value) = split_key_value(item.trim(), path, line_no)?;
        if fields.contains_key(key) {
            return Err(format!(
                "{}:{}: duplicate project dependency field `{}`",
                path.display(),
                line_no,
                key
            ));
        }
        fields.insert(
            key.to_string(),
            parse_inline_value(value.trim(), path, line_no)?,
        );
    }
    if fields.is_empty() {
        return Err(format!(
            "{}:{}: project dependency inline table cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok(fields)
}

/// Parses one inline-table value.
///
/// Inputs:
/// - `value`: trimmed field value source.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - String or string-array inline value.
///
/// Transformation:
/// - Keeps the dependency table grammar intentionally small while admitting
///   Rust feature lists for target-native package metadata.
fn parse_inline_value(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ManifestInlineValue, String> {
    if value.starts_with('[') {
        parse_string_array(value, path, line_no).map(ManifestInlineValue::StringArray)
    } else {
        parse_string(value, path, line_no).map(ManifestInlineValue::String)
    }
}

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
fn validate_dependency_alias(alias: &str, path: &Path, line_no: usize) -> Result<(), String> {
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
fn validate_package_name(name: &str, path: &Path) -> Result<(), String> {
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
fn validate_package_namespace(namespace: &str, path: &Path) -> Result<(), String> {
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
fn validate_package_version(version: &str, path: &Path) -> Result<(), String> {
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

/// Parses a double-quoted manifest string.
///
/// Inputs:
/// - `value`: trimmed value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Unescaped string value.
///
/// Transformation:
/// - Accepts a small escape subset needed by package names and source roots:
///   `\"`, `\\`, `\n`, `\r`, and `\t`.
fn parse_string(value: &str, path: &Path, line_no: usize) -> Result<String, String> {
    let inner = value
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .ok_or_else(|| {
            format!(
                "{}:{}: project manifest value must be a double-quoted string",
                path.display(),
                line_no
            )
        })?;
    unescape_string(inner, path, line_no)
}

/// Parses an array of double-quoted manifest strings.
///
/// Inputs:
/// - `value`: trimmed value text.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Ordered string entries.
///
/// Transformation:
/// - Parses the reviewed one-line `[ "a", "b" ]` subset and rejects empty
///   arrays so source-root discovery remains explicit.
fn parse_string_array(value: &str, path: &Path, line_no: usize) -> Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|text| text.strip_suffix(']'))
        .ok_or_else(|| {
            format!(
                "{}:{}: project manifest value must be an array of strings",
                path.display(),
                line_no
            )
        })?;
    let mut entries = Vec::new();
    for item in split_array_items(inner, path, line_no)? {
        entries.push(parse_string(item.trim(), path, line_no)?);
    }
    if entries.is_empty() {
        return Err(format!(
            "{}:{}: project manifest string array cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok(entries)
}

/// Splits a manifest array body into item slices.
///
/// Inputs:
/// - `inner`: text inside `[` and `]`.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Slices for each array entry.
///
/// Transformation:
/// - Splits on commas outside strings and nested array brackets, then rejects
///   trailing empty entries.
fn split_array_items<'a>(
    inner: &'a str,
    path: &Path,
    line_no: usize,
) -> Result<Vec<&'a str>, String> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut bracket_depth = 0usize;
    for (index, ch) in inner.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '[' if !in_string => bracket_depth += 1,
            ']' if !in_string => {
                bracket_depth = bracket_depth.checked_sub(1).ok_or_else(|| {
                    format!(
                        "{}:{}: project manifest string array has an unmatched closing bracket",
                        path.display(),
                        line_no
                    )
                })?;
            }
            ',' if !in_string && bracket_depth == 0 => {
                let item = inner[start..index].trim();
                if item.is_empty() {
                    return Err(format!(
                        "{}:{}: project manifest string array contains an empty item",
                        path.display(),
                        line_no
                    ));
                }
                items.push(item);
                start = index + 1;
            }
            _ => {}
        }
    }
    if in_string {
        return Err(format!(
            "{}:{}: project manifest string array has an unterminated string",
            path.display(),
            line_no
        ));
    }
    if bracket_depth != 0 {
        return Err(format!(
            "{}:{}: project manifest string array has an unclosed nested array",
            path.display(),
            line_no
        ));
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() {
        items.push(tail);
    }
    Ok(items)
}

/// Unescapes supported manifest string escapes.
///
/// Inputs:
/// - `inner`: text inside double quotes.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Unescaped string.
///
/// Transformation:
/// - Converts the reviewed escape subset and rejects unknown or dangling
///   escapes so manifest text cannot be misread.
fn unescape_string(inner: &str, path: &Path, line_no: usize) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let escaped = chars.next().ok_or_else(|| {
            format!(
                "{}:{}: project manifest string has a dangling escape",
                path.display(),
                line_no
            )
        })?;
        match escaped {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            other => {
                return Err(format!(
                    "{}:{}: unsupported project manifest string escape `\\{}`",
                    path.display(),
                    line_no,
                    other
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "project_manifest_test.rs"]
mod project_manifest_test;
