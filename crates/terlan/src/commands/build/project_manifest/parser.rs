use super::*;

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
    let mut manifest = parse_project_manifest(&source, path)?;
    if let Some(accelerator) = manifest.accelerator.as_mut() {
        let package_root = path.parent().unwrap_or_else(|| Path::new("."));
        let contract = crate::compiler::accelerator::AcceleratorDescriptor::read(
            package_root,
            Path::new(&accelerator.descriptor),
        )?;
        if contract.schema != accelerator.schema {
            return Err(format!(
                "{}: [accelerator] schema `{}` does not match descriptor schema `{}`",
                path.display(),
                accelerator.schema,
                contract.schema
            ));
        }
        accelerator.contract = Some(contract);
    }
    Ok(manifest)
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
///   - optional publication metadata: `description`, `license`, `repository`,
///     `compiler`, and `links`
///   - optional `[build] source_roots = ["src", "lib"]`
///   - optional `[build] artifact = "terlan-vm"`
///   - optional `[web.assets] directory = "assets"`
///   - optional `[web.assets] public_path = "/assets"`
///   - optional `[web.assets] inline_limit = 8192`
///   - optional `[web.assets] rsbuild_config = "rsbuild.config.mjs"`
///   - optional `[scripts] seed = "scripts/Seed.terls"`
///   - optional `[server] profile = "development" | "test" | "staging" | "production"`
///   - optional `[serve]` runtime listener and bounded-request overrides
///   - optional `[server.tls] mode = "auto" | "manual" | "internal"`
///   - optional `[server.tls]` mode-specific certificate, ACME, and internal
///     development CA metadata
///   - `[dependencies] name = { path = "../name" }`
///   - `[target.js.dependencies] zod = { npm = "zod", version = "3.25.0" }`
///   - `[target.rust.dependencies] serde = { cargo = "serde", version = "1.0.0" }`
///   - optional Rust feature flags:
///     `{ cargo = "polars", version = "0.54.4", features = ["lazy", "csv"] }`
/// - Defaults source roots to `["src"]` and artifact to `terlan-vm` when
///   `[build]` omits them.
pub(crate) fn parse_project_manifest(source: &str, path: &Path) -> Result<ProjectManifest, String> {
    let mut section = ProjectManifestSection::Root;
    let mut package_name = None;
    let mut package_version = None;
    let mut package_namespace = None;
    let mut package_description = None;
    let mut package_license = None;
    let mut package_repository = None;
    let mut package_compiler = None;
    let mut package_links = None;
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
    let mut scripts = Vec::new();
    let mut server_profile = None;
    let mut server_tls = ProjectServerTlsBuilder::default();
    let mut native_rust_crate = None;
    let mut native_rust_path = None;
    let mut native_rust_helper = None;
    let mut native_rust_helper_env = None;
    let mut native_rust_features = None;
    let mut accelerator = ProjectAcceleratorBuilder::default();
    let mut deployment = ProjectDeploymentBuilder::default();
    let mut deploy_health = ProjectDeployHealthBuilder::default();
    let mut deploy_resources = ProjectDeployResourcesBuilder::default();
    let mut dependencies = Vec::new();

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
                ProjectManifestSection::Deploy => deployment.seen = true,
                ProjectManifestSection::DeployHealth => deploy_health.seen = true,
                ProjectManifestSection::DeployResources => deploy_resources.seen = true,
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
                "description" => {
                    package_description = Some(parse_package_metadata_string(
                        "description",
                        value,
                        path,
                        line_no,
                    )?);
                }
                "license" => {
                    package_license = Some(parse_package_metadata_string(
                        "license", value, path, line_no,
                    )?);
                }
                "repository" => {
                    package_repository = Some(parse_package_metadata_string(
                        "repository",
                        value,
                        path,
                        line_no,
                    )?);
                }
                "compiler" => {
                    package_compiler = Some(parse_package_metadata_string(
                        "compiler", value, path, line_no,
                    )?);
                }
                "links" => {
                    let links = parse_string_array(value, path, line_no)?;
                    validate_package_links(&links, path, line_no)?;
                    package_links = Some(links);
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
            ProjectManifestSection::Scripts => {
                if scripts
                    .iter()
                    .any(|script: &ProjectScript| script.name == key)
                {
                    return Err(format!(
                        "{}:{}: [scripts] alias `{key}` is declared more than once",
                        path.display(),
                        line_no
                    ));
                }
                scripts.push(parse_script_entry(key, value, path, line_no)?);
            }
            ProjectManifestSection::Server => match key {
                "profile" => {
                    server_profile = Some(parse_server_profile(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [server] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::Serve => match key {
                "host" | "protocol" | "telemetry" | "log_format" | "certificate_cache" => {
                    let _ = parse_string(value, path, line_no)?;
                }
                "allow_public" => {
                    let _ = parse_bool(value, path, line_no)?;
                }
                "port" | "poll_ms" | "max_connections" | "max_request_bytes" | "max_body_bytes"
                | "max_header_bytes" | "request_timeout_ms" | "idle_timeout_ms"
                | "queue_capacity" | "handler_pool_size" | "shutdown_grace_ms" => {
                    let _ = parse_non_negative_u64(value, path, line_no)?;
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [serve] key `{}`",
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
            ProjectManifestSection::Accelerator => match key {
                "schema" => {
                    accelerator.schema = Some(parse_non_negative_u64(value, path, line_no)?);
                }
                "descriptor" => {
                    accelerator.descriptor = Some(parse_string(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [accelerator] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::Deploy => match key {
                "environment" => {
                    deployment.environment = Some(parse_string_array(value, path, line_no)?);
                }
                "secrets" => {
                    deployment.secrets = Some(parse_string_array(value, path, line_no)?);
                }
                "migrations" => {
                    deployment.migrations = Some(parse_string_array(value, path, line_no)?);
                }
                "outbound_network" => {
                    deployment.outbound_network = Some(parse_string_array(value, path, line_no)?);
                }
                "rollback" => {
                    deployment.rollback = Some(parse_rollback_compatibility(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [deploy] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::DeployHealth => match key {
                "path" => {
                    deploy_health.path = Some(parse_string(value, path, line_no)?);
                }
                "interval_secs" => {
                    deploy_health.interval_secs =
                        Some(parse_non_negative_u64(value, path, line_no)?);
                }
                "timeout_secs" => {
                    deploy_health.timeout_secs =
                        Some(parse_non_negative_u64(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [deploy.health] key `{}`",
                        path.display(),
                        line_no,
                        key
                    ));
                }
            },
            ProjectManifestSection::DeployResources => match key {
                "cpu_millis" => {
                    deploy_resources.cpu_millis =
                        Some(parse_non_negative_u64(value, path, line_no)?);
                }
                "memory_mb" => {
                    deploy_resources.memory_mb =
                        Some(parse_non_negative_u64(value, path, line_no)?);
                }
                "processes" => {
                    deploy_resources.processes =
                        Some(parse_non_negative_u64(value, path, line_no)?);
                }
                _ => {
                    return Err(format!(
                        "{}:{}: unsupported [deploy.resources] key `{}`",
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
    validate_source_roots(path, &source_roots)?;
    let artifact = artifact.unwrap_or(ProjectArtifactKind::TerlanVm);
    let wasm_target = finish_wasm_target(
        path,
        artifact,
        ParsedWasmTarget {
            seen: wasm_target_seen,
            profile: wasm_profile,
            exports: wasm_exports,
            bridge: wasm_bridge,
            capabilities: wasm_capabilities,
            world: wasm_world,
            validation_engine: wasm_validation_engine,
        },
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
    validate_server_profile_defaults(path, server_profile, server_tls.as_ref())?;
    let native_rust = finish_native_rust(
        path,
        native_rust_crate,
        native_rust_path,
        native_rust_helper,
        native_rust_helper_env,
        native_rust_features,
    )?;
    let accelerator = accelerator.finish(path)?;
    let deployment = deployment.finish(deploy_health, deploy_resources, path)?;

    Ok(ProjectManifest {
        package: ProjectPackage {
            name,
            version,
            namespace: package_namespace,
            description: package_description,
            license: package_license,
            repository: package_repository,
            compiler: package_compiler,
            links: package_links.unwrap_or_default(),
        },
        source_roots,
        artifact,
        scripts,
        wasm_target,
        wasi_target,
        web_assets,
        server_profile,
        server_tls,
        native_rust,
        accelerator,
        deployment,
        dependencies,
    })
}

/// Parses one non-empty package publication metadata field.
fn parse_package_metadata_string(
    key: &str,
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<String, String> {
    let value = parse_string(value, path, line_no)?;
    if value.trim().is_empty() {
        return Err(format!(
            "{}:{}: [package] `{key}` cannot be empty",
            path.display(),
            line_no
        ));
    }
    Ok(value)
}

/// Validates package documentation/project link metadata.
fn validate_package_links(links: &[String], path: &Path, line_no: usize) -> Result<(), String> {
    if links.is_empty() {
        return Err(format!(
            "{}:{}: [package] `links` must contain at least one URL",
            path.display(),
            line_no
        ));
    }
    let mut seen = BTreeSet::new();
    for link in links {
        if link.trim().is_empty() {
            return Err(format!(
                "{}:{}: [package] `links` cannot contain empty entries",
                path.display(),
                line_no
            ));
        }
        if !seen.insert(link) {
            return Err(format!(
                "{}:{}: [package] `links` contains duplicate `{link}`",
                path.display(),
                line_no
            ));
        }
    }
    Ok(())
}

/// Validates profile-specific server defaults.
///
/// Inputs:
/// - `path`: manifest path used in diagnostics.
/// - `server_profile`: optional typed deployment profile.
/// - `server_tls`: optional parsed TLS configuration.
///
/// Output:
/// - `Ok(())` when profile-specific defaults are safe.
/// - `Err(String)` when a production profile uses development-only defaults.
///
/// Transformation:
/// - Rejects internal TLS under production before runtime startup can inherit a
///   local-development certificate policy.
fn validate_server_profile_defaults(
    path: &Path,
    server_profile: Option<ProjectServerProfile>,
    server_tls: Option<&ProjectServerTls>,
) -> Result<(), String> {
    if matches!(server_profile, Some(ProjectServerProfile::Production))
        && matches!(
            server_tls.map(|tls| tls.mode),
            Some(ProjectServerTlsMode::Internal)
        )
    {
        return Err(format!(
            "{}: project manifest [server] profile production cannot use [server.tls] mode internal",
            path.display()
        ));
    }
    Ok(())
}

/// Validates package source roots before build or package-test execution.
///
/// Inputs:
/// - `path`: manifest path used in diagnostics.
/// - `source_roots`: parsed `[build] source_roots` values.
///
/// Output:
/// - `Ok(())` when roots are deterministic, package-relative paths.
/// - `Err(String)` when a root could escape or ambiguously alias the package.
///
/// Transformation:
/// - Rejects empty lists, empty entries, absolute paths, parent traversal,
///   current-directory roots, duplicate roots, and roots with surrounding
///   whitespace. Package execution can then resolve source roots relative to
///   the manifest without silently running code from a different workspace.
fn validate_source_roots(path: &Path, source_roots: &[String]) -> Result<(), String> {
    if source_roots.is_empty() {
        return Err(format!(
            "{}: project manifest [build] source_roots must contain at least one package-relative entry",
            path.display()
        ));
    }

    let mut seen = BTreeSet::new();
    for root in source_roots {
        if root.trim().is_empty() {
            return Err(format!(
                "{}: project manifest [build] source_roots cannot contain empty entries",
                path.display()
            ));
        }
        if root != root.trim() {
            return Err(format!(
                "{}: project manifest [build] source_root `{root}` cannot contain leading or trailing whitespace",
                path.display()
            ));
        }
        let root_path = Path::new(root);
        if root_path.is_absolute() {
            return Err(format!(
                "{}: project manifest [build] source_root `{root}` must be package-relative",
                path.display()
            ));
        }
        if root == "."
            || root_path
                .components()
                .any(|part| part == Component::ParentDir)
        {
            return Err(format!(
                "{}: project manifest [build] source_root `{root}` cannot use current-directory or parent traversal",
                path.display()
            ));
        }
        if !seen.insert(root.as_str()) {
            return Err(format!(
                "{}: project manifest [build] source_root `{root}` is declared more than once",
                path.display()
            ));
        }
    }

    Ok(())
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
    Scripts,
    Server,
    Serve,
    ServerTls,
    NativeRust,
    Accelerator,
    Deploy,
    DeployHealth,
    DeployResources,
    Dependencies,
    TargetDependencies(ProjectTarget),
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
///   `[web.assets]`, `[scripts]`, `[server]`, `[serve]`, `[server.tls]`, and
///   `[target.<name>.dependencies]`
///   section headers.
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
        "scripts" => Ok(ProjectManifestSection::Scripts),
        "server" => Ok(ProjectManifestSection::Server),
        "serve" => Ok(ProjectManifestSection::Serve),
        "server.tls" => Ok(ProjectManifestSection::ServerTls),
        "native.rust" => Ok(ProjectManifestSection::NativeRust),
        "accelerator" => Ok(ProjectManifestSection::Accelerator),
        "deploy" => Ok(ProjectManifestSection::Deploy),
        "deploy.health" => Ok(ProjectManifestSection::DeployHealth),
        "deploy.resources" => Ok(ProjectManifestSection::DeployResources),
        "dependencies" => Ok(ProjectManifestSection::Dependencies),
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

/// Parses one project-local script alias.
///
/// Inputs:
/// - `alias`: manifest key from `[scripts]`.
/// - `value`: manifest value expected to be a package-relative `.terls` path.
/// - `path`: manifest path used in diagnostics.
/// - `line_no`: 1-based line number used in diagnostics.
///
/// Output:
/// - Parsed `ProjectScript` alias.
///
/// Transformation:
/// - Narrows free-form manifest entries to stable aliases and safe relative
///   Terlan source paths so script execution stays package-local.
fn parse_script_entry(
    alias: &str,
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectScript, String> {
    validate_script_alias(alias, path, line_no)?;
    let script_path = parse_string(value, path, line_no)?;
    validate_script_path(&script_path, path, line_no)?;
    Ok(ProjectScript {
        name: alias.to_string(),
        path: script_path,
    })
}

/// Validates a project-local script alias.
fn validate_script_alias(alias: &str, path: &Path, line_no: usize) -> Result<(), String> {
    if alias.trim().is_empty() {
        return Err(format!(
            "{}:{}: [scripts] alias cannot be empty",
            path.display(),
            line_no
        ));
    }
    if alias != alias.trim() {
        return Err(format!(
            "{}:{}: [scripts] alias `{alias}` cannot contain leading or trailing whitespace",
            path.display(),
            line_no
        ));
    }
    let valid = alias.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
    });
    if !valid {
        return Err(format!(
            "{}:{}: [scripts] alias `{alias}` may contain only lowercase ASCII letters, digits, `_`, `-`, or `.`",
            path.display(),
            line_no
        ));
    }
    Ok(())
}
