use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

mod config;
mod inline_table;
mod model;
mod native_rust;
mod strings;
mod targets;
mod validation;

use config::{
    parse_bool, parse_non_negative_u64, parse_server_tls_mode, parse_server_tls_provider,
    ProjectServerTlsBuilder, ProjectWebAssetsBuilder,
};
use inline_table::{
    expect_inline_string_array_field, expect_inline_string_field, parse_inline_table,
    ProjectManifestInlineValue,
};
pub(crate) use model::{
    ProjectArtifactKind, ProjectDependency, ProjectDependencyScope, ProjectDependencySource,
    ProjectErlangPackageAdapter, ProjectManifest, ProjectNativeRust, ProjectPackage,
    ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider, ProjectTarget,
    ProjectWasiProfile, ProjectWasmProfile, ProjectWebAssets,
};
#[cfg(test)]
pub(crate) use model::{ProjectWasiTarget, ProjectWasmTarget};
use native_rust::finish_native_rust;
use strings::{parse_string, parse_string_array};
use targets::{finish_wasi_target, finish_wasm_target, parse_wasi_profile, parse_wasm_profile};
use validation::{
    validate_dependency_alias, validate_package_name, validate_package_namespace,
    validate_package_version,
};

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
///   - optional `[web.assets] rsbuild_config = "rsbuild.config.mjs"`
///   - optional `[server.tls] mode = "auto" | "manual" | "internal"`
///   - optional `[server.tls]` mode-specific certificate, ACME, and internal
///     development CA metadata
///   - `[dependencies] name = { path = "../name" }`
///   - `[target.erlang.dependencies] cowboy = { hex = "cowboy", version = "2.12.0" }`
///   - `[target.js.dependencies] zod = { npm = "zod", version = "3.25.0" }`
///   - `[target.rust.dependencies] serde = { cargo = "serde", version = "1.0.0" }`
///   - optional Rust feature flags:
///     `{ cargo = "polars", version = "0.54.4", features = ["lazy", "csv"] }`
/// - Defaults source roots to `["src"]` and artifact to `beam-thin` when
///   `[build]` omits them.
pub(crate) fn parse_project_manifest(source: &str, path: &Path) -> Result<ProjectManifest, String> {
    let mut section = ProjectManifestSection::Root;
    let mut package_name = None;
    let mut package_version = None;
    let mut package_namespace = None;
    let mut source_roots = None;
    let mut artifact = None;
    let mut wasm_target_seen = false;
    let mut wasm_profile: Option<ProjectWasmProfile> = None;
    let mut wasm_exports: Option<Vec<String>> = None;
    let mut wasm_bridge: Option<String> = None;
    let mut wasm_capabilities: Option<Vec<String>> = None;
    let mut wasm_world: Option<String> = None;
    let mut wasm_validation_engine: Option<String> = None;
    let mut wasi_target_seen = false;
    let mut wasi_profile: Option<ProjectWasiProfile> = None;
    let mut wasi_world: Option<String> = None;
    let mut wasi_capabilities: Option<Vec<String>> = None;
    let mut wasi_validation_engine: Option<String> = None;
    let mut web_assets = ProjectWebAssetsBuilder::default();
    let mut server_tls = ProjectServerTlsBuilder::default();
    let mut native_rust_crate = None;
    let mut native_rust_path = None;
    let mut native_rust_helper = None;
    let mut native_rust_helper_env = None;
    let mut native_rust_features = None;
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
            match section {
                ProjectManifestSection::TargetWasm => wasm_target_seen = true,
                ProjectManifestSection::TargetWasi => wasi_target_seen = true,
                _ => {}
            }
            continue;
        }

        let (key, value) = split_key_value(line, path, line_no)?;
        match section {
            ProjectManifestSection::Root => {
                return Err(format!(
                    "{}:{}: manifest keys must appear inside a supported project manifest section",
                    path.display(),
                    line_no
                ));
            }
            ProjectManifestSection::Package => match key {
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
            ProjectManifestSection::Build => match key {
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
            ProjectManifestSection::WebAssets => match key {
                "directory" => {
                    web_assets.directory = Some(parse_string(value, path, line_no)?);
                }
                "public_path" => {
                    web_assets.public_path = Some(parse_string(value, path, line_no)?);
                }
                "inline_limit" => {
                    web_assets.inline_limit = Some(parse_non_negative_u64(value, path, line_no)?);
                }
                "rsbuild_config" => {
                    web_assets.rsbuild_config = Some(parse_string(value, path, line_no)?);
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
            ProjectManifestSection::ServerTls => match key {
                "mode" => {
                    server_tls.mode = Some(parse_server_tls_mode(value, path, line_no)?);
                }
                "domains" => {
                    server_tls.domains = Some(parse_string_array(value, path, line_no)?);
                }
                "email" => {
                    server_tls.email = Some(parse_string(value, path, line_no)?);
                }
                "primary_provider" => {
                    server_tls.primary_provider =
                        Some(parse_server_tls_provider(value, path, line_no)?);
                }
                "fallback_provider" => {
                    server_tls.fallback_provider =
                        Some(parse_server_tls_provider(value, path, line_no)?);
                }
                "cert" => {
                    server_tls.cert = Some(parse_string(value, path, line_no)?);
                }
                "key" => {
                    server_tls.key = Some(parse_string(value, path, line_no)?);
                }
                "passphrase_env" => {
                    server_tls.passphrase_env = Some(parse_string(value, path, line_no)?);
                }
                "ca" => {
                    server_tls.ca = Some(parse_string(value, path, line_no)?);
                }
                "server_name" => {
                    server_tls.server_name = Some(parse_string(value, path, line_no)?);
                }
                "trust_local" => {
                    server_tls.trust_local = Some(parse_bool(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [server.tls] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::NativeRust => match key {
                "crate" => {
                    native_rust_crate = Some(parse_string(value, path, line_no)?);
                }
                "path" => {
                    native_rust_path = Some(parse_string(value, path, line_no)?);
                }
                "helper" => {
                    native_rust_helper = Some(parse_string(value, path, line_no)?);
                }
                "helper_env" => {
                    native_rust_helper_env = Some(parse_string(value, path, line_no)?);
                }
                "features" => {
                    native_rust_features = Some(parse_string_array(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [native.rust] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::Dependencies => {
                dependencies.push(parse_dependency_entry(
                    ProjectDependencyScope::Local,
                    key,
                    value,
                    path,
                    line_no,
                )?);
            }
            ProjectManifestSection::TargetDependencies(target) => {
                dependencies.push(parse_dependency_entry(
                    ProjectDependencyScope::Target(target),
                    key,
                    value,
                    path,
                    line_no,
                )?);
            }
            ProjectManifestSection::TargetErlangPackage => match key {
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
            ProjectManifestSection::TargetWasm => match key {
                "profile" => {
                    wasm_profile = Some(parse_wasm_profile(value, path, line_no)?);
                }
                "exports" => {
                    wasm_exports = Some(parse_string_array(value, path, line_no)?);
                }
                "bridge" => {
                    wasm_bridge = Some(parse_string(value, path, line_no)?);
                }
                "capabilities" => {
                    wasm_capabilities = Some(parse_string_array(value, path, line_no)?);
                }
                "world" => {
                    wasm_world = Some(parse_string(value, path, line_no)?);
                }
                "validation_engine" => {
                    wasm_validation_engine = Some(parse_string(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [target.wasm] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::TargetWasi => match key {
                "profile" => {
                    wasi_profile = Some(parse_wasi_profile(value, path, line_no)?);
                }
                "world" => {
                    wasi_world = Some(parse_string(value, path, line_no)?);
                }
                "capabilities" => {
                    wasi_capabilities = Some(parse_string_array(value, path, line_no)?);
                }
                "validation_engine" => {
                    wasi_validation_engine = Some(parse_string(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [target.wasi] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::IntegrationFlow => match key {
                "traits" | "host" | "port" | "compose_service" | "migrations" | "wait_secs"
                | "http_checks" | "websocket_checks" => {}
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [integration.*] key `{}`",
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
    let wasm_target = finish_wasm_target(
        path,
        artifact,
        wasm_target_seen,
        wasm_profile,
        wasm_exports,
        wasm_bridge,
        wasm_capabilities,
        wasm_world,
        wasm_validation_engine,
    )?;
    let wasi_target = finish_wasi_target(
        path,
        artifact,
        wasi_target_seen,
        wasi_profile,
        wasi_world,
        wasi_capabilities,
        wasi_validation_engine,
    )?;
    let web_assets = web_assets.finish(path)?;
    let server_tls = server_tls.finish(path)?;
    let native_rust = finish_native_rust(
        path,
        native_rust_crate,
        native_rust_path,
        native_rust_helper,
        native_rust_helper_env,
        native_rust_features,
    )?;

    Ok(ProjectManifest {
        package: ProjectPackage {
            name,
            version,
            namespace: package_namespace,
        },
        source_roots,
        artifact,
        wasm_target,
        wasi_target,
        web_assets,
        server_tls,
        native_rust,
        dependencies,
        erlang_package_adapter,
    })
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
enum ProjectManifestSection {
    Root,
    Package,
    Build,
    WebAssets,
    ServerTls,
    NativeRust,
    Dependencies,
    TargetDependencies(ProjectTarget),
    TargetErlangPackage,
    TargetWasm,
    TargetWasi,
    IntegrationFlow,
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
/// - Supported `ProjectManifestSection`.
///
/// Transformation:
/// - Accepts exact `[package]`, `[build]`, `[dependencies]`,
///   `[web.assets]`, `[server.tls]`, `[target.<name>.dependencies]`, and
///   `[target.erlang.package]` section headers.
fn parse_section(
    line: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectManifestSection, String> {
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
        "package" => Ok(ProjectManifestSection::Package),
        "build" => Ok(ProjectManifestSection::Build),
        "web.assets" => Ok(ProjectManifestSection::WebAssets),
        "server.tls" => Ok(ProjectManifestSection::ServerTls),
        "native.rust" => Ok(ProjectManifestSection::NativeRust),
        "dependencies" => Ok(ProjectManifestSection::Dependencies),
        "target.erlang.package" => Ok(ProjectManifestSection::TargetErlangPackage),
        "target.wasm" => Ok(ProjectManifestSection::TargetWasm),
        "target.wasi" => Ok(ProjectManifestSection::TargetWasi),
        other => {
            if other.starts_with("integration.") && other["integration.".len()..].trim().is_empty()
            {
                Err(format!(
                    "{}:{}: unsupported project manifest section `{}`",
                    path.display(),
                    line_no,
                    other
                ))
            } else if other.starts_with("integration.") {
                Ok(ProjectManifestSection::IntegrationFlow)
            } else if let Some(target) = parse_target_dependency_section(other) {
                Ok(ProjectManifestSection::TargetDependencies(target))
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
        "wasm-core" => Ok(ProjectArtifactKind::WasmCore),
        "wasm-browser" => Ok(ProjectArtifactKind::WasmBrowser),
        "wasm-component" => Ok(ProjectArtifactKind::WasmComponent),
        "wasi-cli" => Ok(ProjectArtifactKind::WasiCli),
        "wasi-http" => Ok(ProjectArtifactKind::WasiHttp),
        "wasi-worker" => Ok(ProjectArtifactKind::WasiWorker),
        other => Err(format!(
            "{}:{}: unsupported [build] artifact `{}`; supported artifacts: beam-thin, library, wasm-core, wasm-browser, wasm-component, wasi-cli, wasi-http, wasi-worker",
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
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
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
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
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
    fields: &BTreeMap<String, ProjectManifestInlineValue>,
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

#[cfg(test)]
#[path = "project_manifest_test.rs"]
mod project_manifest_test;
