//! Typed, deterministic configuration for the VM-owned HTTP serve boundary.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::args::{ServeArgs, DEFAULT_POLL_MS, DEFAULT_SERVE_HOST, DEFAULT_SERVE_PORT};
use super::manifest;

pub(super) const SERVE_CONFIG_SCHEMA: &str = "terlan-vm-serve-config-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ServeProtocol {
    Http1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ServeTelemetry {
    Off,
    Basic,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ServeLogFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EffectiveServeConfig {
    pub(super) schema: &'static str,
    pub(super) fingerprint: String,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) protocol: ServeProtocol,
    pub(super) tls_mode: String,
    pub(super) static_root: PathBuf,
    pub(super) certificate_cache: PathBuf,
    pub(super) poll_ms: u64,
    pub(super) max_connections: u64,
    pub(super) max_request_bytes: u64,
    pub(super) max_body_bytes: u64,
    pub(super) max_header_bytes: u64,
    pub(super) request_timeout_ms: u64,
    pub(super) idle_timeout_ms: u64,
    pub(super) queue_capacity: u64,
    pub(super) handler_pool_size: u64,
    pub(super) telemetry: ServeTelemetry,
    pub(super) log_format: ServeLogFormat,
    pub(super) shutdown_grace_ms: u64,
    pub(super) allow_public: bool,
    pub(super) profile: String,
    pub(super) sources: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ServeOverrides {
    host: Option<String>,
    port: Option<u16>,
    protocol: Option<String>,
    allow_public: Option<bool>,
    poll_ms: Option<u64>,
    max_connections: Option<u64>,
    max_request_bytes: Option<u64>,
    max_body_bytes: Option<u64>,
    max_header_bytes: Option<u64>,
    request_timeout_ms: Option<u64>,
    idle_timeout_ms: Option<u64>,
    queue_capacity: Option<u64>,
    handler_pool_size: Option<u64>,
    telemetry: Option<String>,
    log_format: Option<String>,
    shutdown_grace_ms: Option<u64>,
    certificate_cache: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ServeManifest {
    serve: ServeOverrides,
    server: ServerSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ServerSection {
    profile: Option<String>,
    tls: TlsSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TlsSection {
    mode: Option<String>,
}

#[derive(Debug, Clone)]
struct MutableConfig {
    host: String,
    port: u16,
    protocol: String,
    allow_public: bool,
    poll_ms: u64,
    max_connections: u64,
    max_request_bytes: u64,
    max_body_bytes: u64,
    max_header_bytes: u64,
    request_timeout_ms: u64,
    idle_timeout_ms: u64,
    queue_capacity: u64,
    handler_pool_size: u64,
    telemetry: String,
    log_format: String,
    shutdown_grace_ms: u64,
    certificate_cache: PathBuf,
    profile: String,
    tls_mode: String,
    sources: BTreeMap<String, String>,
}

impl MutableConfig {
    fn defaults(web_root: &Path) -> Self {
        let fields = [
            "host",
            "port",
            "protocol",
            "allow_public",
            "poll_ms",
            "max_connections",
            "max_request_bytes",
            "max_body_bytes",
            "max_header_bytes",
            "request_timeout_ms",
            "idle_timeout_ms",
            "queue_capacity",
            "handler_pool_size",
            "telemetry",
            "log_format",
            "shutdown_grace_ms",
            "certificate_cache",
            "profile",
            "tls_mode",
        ];
        Self {
            host: DEFAULT_SERVE_HOST.to_string(),
            port: DEFAULT_SERVE_PORT,
            protocol: "http1".to_string(),
            allow_public: false,
            poll_ms: DEFAULT_POLL_MS,
            max_connections: 16_384,
            max_request_bytes: 10 * 1024 * 1024,
            max_body_bytes: 8 * 1024 * 1024,
            max_header_bytes: 64 * 1024,
            request_timeout_ms: 30_000,
            idle_timeout_ms: 60_000,
            queue_capacity: 4_096,
            handler_pool_size: available_parallelism(),
            telemetry: "basic".to_string(),
            log_format: "text".to_string(),
            shutdown_grace_ms: 10_000,
            certificate_cache: web_root.join(".terlan/certificates"),
            profile: "development".to_string(),
            tls_mode: "plain".to_string(),
            sources: fields
                .into_iter()
                .map(|field| (field.to_string(), "default".to_string()))
                .collect(),
        }
    }

    fn mark(&mut self, field: &str, source: &str) {
        self.sources.insert(field.to_string(), source.to_string());
    }

    fn apply(&mut self, value: ServeOverrides, source: &str) {
        macro_rules! apply {
            ($field:ident) => {
                if let Some(value) = value.$field {
                    self.$field = value;
                    self.mark(stringify!($field), source);
                }
            };
        }
        apply!(host);
        apply!(port);
        apply!(protocol);
        apply!(allow_public);
        apply!(poll_ms);
        apply!(max_connections);
        apply!(max_request_bytes);
        apply!(max_body_bytes);
        apply!(max_header_bytes);
        apply!(request_timeout_ms);
        apply!(idle_timeout_ms);
        apply!(queue_capacity);
        apply!(handler_pool_size);
        apply!(telemetry);
        apply!(log_format);
        apply!(shutdown_grace_ms);
        if let Some(value) = value.certificate_cache {
            self.certificate_cache = PathBuf::from(value);
            self.mark("certificate_cache", source);
        }
    }
}

pub(super) fn resolve_effective_serve_config(
    args: &ServeArgs,
) -> super::ServeResult<EffectiveServeConfig> {
    resolve_effective_serve_config_with_env(args, std::env::vars())
}

pub(super) fn resolve_effective_serve_config_with_env(
    args: &ServeArgs,
    environment: impl IntoIterator<Item = (String, String)>,
) -> super::ServeResult<EffectiveServeConfig> {
    let project_root = manifest::adjacent_project_root(&args.web_root);
    let manifest_path = project_root.as_ref().map(|root| root.join("terlan.toml"));
    let manifest_config = manifest_path
        .as_deref()
        .filter(|path| path.is_file())
        .map(read_manifest)
        .transpose()?
        .unwrap_or_default();
    let mut config = MutableConfig::defaults(&args.web_root);
    config.apply(manifest_config.serve, "manifest");
    if let Some(profile) = manifest_config.server.profile {
        config.profile = profile;
        config.mark("profile", "manifest");
    }
    if let Some(tls_mode) = manifest_config.server.tls.mode {
        config.tls_mode = tls_mode;
        config.mark("tls_mode", "manifest");
    }
    config.apply(environment_overrides(environment)?, "environment");
    apply_cli(&mut config, args);
    finish(config, args.web_root.clone(), project_root.as_deref())
}

fn read_manifest(path: &Path) -> super::ServeResult<ServeManifest> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read serve configuration {}: {error}",
            path.display()
        )
    })?;
    Ok(basic_toml::from_str(&source).map_err(|error| {
        format!(
            "cannot parse serve configuration {}: {error}",
            path.display()
        )
    })?)
}

fn environment_overrides(
    environment: impl IntoIterator<Item = (String, String)>,
) -> super::ServeResult<ServeOverrides> {
    let values: BTreeMap<_, _> = environment.into_iter().collect();
    let text = |name: &str| values.get(name).cloned();
    let number = |name: &str| -> Result<Option<u64>, String> {
        text(name)
            .map(|value| {
                value.parse::<u64>().map_err(|_| {
                    format!(
                        "error[serve.config]: {name} expects an unsigned integer, got `{value}`"
                    )
                })
            })
            .transpose()
    };
    let boolean = |name: &str| -> Result<Option<bool>, String> {
        text(name)
            .map(|value| match value.as_str() {
                "1" | "true" => Ok(true),
                "0" | "false" => Ok(false),
                _ => Err(format!(
                    "error[serve.config]: {name} expects true or false, got `{value}`"
                )),
            })
            .transpose()
    };
    Ok(ServeOverrides {
        host: text("TERLAN_SERVE_HOST"),
        port: number("TERLAN_SERVE_PORT")?
            .map(|value| {
                u16::try_from(value)
                    .map_err(|_| "error[serve.config]: TERLAN_SERVE_PORT exceeds u16".to_string())
            })
            .transpose()?,
        protocol: text("TERLAN_SERVE_PROTOCOL"),
        allow_public: boolean("TERLAN_SERVE_ALLOW_PUBLIC")?,
        poll_ms: number("TERLAN_SERVE_POLL_MS")?,
        max_connections: number("TERLAN_SERVE_MAX_CONNECTIONS")?,
        max_request_bytes: number("TERLAN_SERVE_MAX_REQUEST_BYTES")?,
        max_body_bytes: number("TERLAN_SERVE_MAX_BODY_BYTES")?,
        max_header_bytes: number("TERLAN_SERVE_MAX_HEADER_BYTES")?,
        request_timeout_ms: number("TERLAN_SERVE_REQUEST_TIMEOUT_MS")?,
        idle_timeout_ms: number("TERLAN_SERVE_IDLE_TIMEOUT_MS")?,
        queue_capacity: number("TERLAN_SERVE_QUEUE_CAPACITY")?,
        handler_pool_size: number("TERLAN_SERVE_HANDLER_POOL_SIZE")?,
        telemetry: text("TERLAN_SERVE_TELEMETRY"),
        log_format: text("TERLAN_SERVE_LOG_FORMAT"),
        shutdown_grace_ms: number("TERLAN_SERVE_SHUTDOWN_GRACE_MS")?,
        certificate_cache: text("TERLAN_SERVE_CERTIFICATE_CACHE"),
    })
}

fn apply_cli(config: &mut MutableConfig, args: &ServeArgs) {
    if args.overrides.host {
        config.host.clone_from(&args.host);
        config.mark("host", "cli");
    }
    if args.overrides.port {
        config.port = args.port;
        config.mark("port", "cli");
    }
    if args.overrides.poll_ms {
        config.poll_ms = args.poll_ms;
        config.mark("poll_ms", "cli");
    }
    let overrides = &args.overrides;
    let cli = ServeOverrides {
        protocol: overrides.protocol.clone(),
        allow_public: overrides.allow_public.then_some(true),
        max_connections: overrides.max_connections,
        max_request_bytes: overrides.max_request_bytes,
        max_body_bytes: overrides.max_body_bytes,
        max_header_bytes: overrides.max_header_bytes,
        request_timeout_ms: overrides.request_timeout_ms,
        idle_timeout_ms: overrides.idle_timeout_ms,
        queue_capacity: overrides.queue_capacity,
        handler_pool_size: overrides.handler_pool_size,
        telemetry: overrides.telemetry.clone(),
        log_format: overrides.log_format.clone(),
        shutdown_grace_ms: overrides.shutdown_grace_ms,
        ..ServeOverrides::default()
    };
    config.apply(cli, "cli");
}

fn finish(
    config: MutableConfig,
    static_root: PathBuf,
    project_root: Option<&Path>,
) -> super::ServeResult<EffectiveServeConfig> {
    validate(&config, &static_root, project_root)?;
    let protocol = match config.protocol.as_str() {
        "http1" => ServeProtocol::Http1,
        other => return Err(format!("error[serve.config.protocol]: unsupported protocol `{other}`; this runtime supports http1").into()),
    };
    let telemetry = match config.telemetry.as_str() {
        "off" => ServeTelemetry::Off,
        "basic" => ServeTelemetry::Basic,
        "full" => ServeTelemetry::Full,
        other => {
            return Err(format!(
                "error[serve.config.telemetry]: unsupported telemetry mode `{other}`"
            )
            .into())
        }
    };
    let log_format = match config.log_format.as_str() {
        "text" => ServeLogFormat::Text,
        "json" => ServeLogFormat::Json,
        other => {
            return Err(
                format!("error[serve.config.log_format]: unsupported log format `{other}`").into(),
            )
        }
    };
    let mut effective = EffectiveServeConfig {
        schema: SERVE_CONFIG_SCHEMA,
        fingerprint: String::new(),
        host: config.host,
        port: config.port,
        protocol,
        tls_mode: config.tls_mode,
        static_root,
        certificate_cache: resolve_project_path(project_root, config.certificate_cache)?,
        poll_ms: config.poll_ms,
        max_connections: config.max_connections,
        max_request_bytes: config.max_request_bytes,
        max_body_bytes: config.max_body_bytes,
        max_header_bytes: config.max_header_bytes,
        request_timeout_ms: config.request_timeout_ms,
        idle_timeout_ms: config.idle_timeout_ms,
        queue_capacity: config.queue_capacity,
        handler_pool_size: config.handler_pool_size,
        telemetry,
        log_format,
        shutdown_grace_ms: config.shutdown_grace_ms,
        allow_public: config.allow_public,
        profile: config.profile,
        sources: config.sources,
    };
    let encoded = serde_json::to_vec(&effective)
        .map_err(|error| format!("error[serve.config]: encode effective config: {error}"))?;
    effective.fingerprint = format!(
        "sha256:{}",
        Sha256::digest(encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    Ok(effective)
}

fn validate(
    config: &MutableConfig,
    static_root: &Path,
    project_root: Option<&Path>,
) -> super::ServeResult<()> {
    if config.host.trim().is_empty() {
        return Err("error[serve.config.host]: host cannot be empty".into());
    }
    if is_public_bind(&config.host) && !config.allow_public {
        return Err("error[serve.config.public_bind]: public bind requires --allow-public or TERLAN_SERVE_ALLOW_PUBLIC=true".into());
    }
    if !matches!(
        config.profile.as_str(),
        "development" | "test" | "staging" | "production"
    ) {
        return Err(format!(
            "error[serve.config.profile]: unsupported profile `{}`",
            config.profile
        )
        .into());
    }
    if !matches!(
        config.tls_mode.as_str(),
        "plain" | "auto" | "manual" | "internal"
    ) {
        return Err(format!(
            "error[serve.config.tls]: unsupported TLS mode `{}`",
            config.tls_mode
        )
        .into());
    }
    if config.profile == "production" && config.tls_mode == "internal" {
        return Err("error[serve.config.tls]: production cannot use internal TLS".into());
    }
    for (name, value) in [
        ("poll_ms", config.poll_ms),
        ("max_connections", config.max_connections),
        ("max_request_bytes", config.max_request_bytes),
        ("max_body_bytes", config.max_body_bytes),
        ("max_header_bytes", config.max_header_bytes),
        ("request_timeout_ms", config.request_timeout_ms),
        ("idle_timeout_ms", config.idle_timeout_ms),
        ("queue_capacity", config.queue_capacity),
        ("handler_pool_size", config.handler_pool_size),
        ("shutdown_grace_ms", config.shutdown_grace_ms),
    ] {
        if value == 0 {
            return Err(
                format!("error[serve.config.limit]: {name} must be greater than zero").into(),
            );
        }
    }
    if config.max_body_bytes > config.max_request_bytes {
        return Err(
            "error[serve.config.limit]: max_body_bytes cannot exceed max_request_bytes".into(),
        );
    }
    if config.max_header_bytes >= config.max_request_bytes {
        return Err(
            "error[serve.config.limit]: max_header_bytes must be smaller than max_request_bytes"
                .into(),
        );
    }
    if config.queue_capacity > config.max_connections {
        return Err(
            "error[serve.config.backpressure]: queue_capacity cannot exceed max_connections".into(),
        );
    }
    if config.handler_pool_size > config.max_connections {
        return Err(
            "error[serve.config.backpressure]: handler_pool_size cannot exceed max_connections"
                .into(),
        );
    }
    if !static_root.is_dir() {
        return Err(format!(
            "error[serve.config.assets]: static root {} is not a directory",
            static_root.display()
        )
        .into());
    }
    resolve_project_path(project_root, config.certificate_cache.clone())?;
    Ok(())
}

fn resolve_project_path(
    project_root: Option<&Path>,
    value: PathBuf,
) -> super::ServeResult<PathBuf> {
    if value.is_absolute() {
        return Ok(value);
    }
    let Some(root) = project_root else {
        return Ok(value);
    };
    if value
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(
            "error[serve.config.path]: certificate cache cannot escape the project root".into(),
        );
    }
    Ok(root.join(value))
}

fn is_public_bind(host: &str) -> bool {
    matches!(host, "0.0.0.0" | "::" | "[::]")
}

fn available_parallelism() -> u64 {
    std::thread::available_parallelism()
        .map(|width| width.get() as u64)
        .unwrap_or(1)
}

pub(super) fn write_effective_serve_config(
    config: &EffectiveServeConfig,
    web_root: &Path,
) -> super::ServeResult<PathBuf> {
    let artifact_root =
        manifest::adjacent_project_root(web_root).unwrap_or_else(|| web_root.to_path_buf());
    let path = artifact_root.join("build/artifacts/serve-effective-config.json");
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("error[serve.config]: encode artifact: {error}"))?;
    atomic_write(&path, &bytes)?;
    Ok(path)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> super::ServeResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "error[serve.config]: artifact has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("error[serve.config]: create {}: {error}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(|error| {
        format!(
            "error[serve.config]: write {}: {error}",
            temporary.display()
        )
    })?;
    Ok(fs::rename(&temporary, path)
        .map_err(|error| format!("error[serve.config]: publish {}: {error}", path.display()))?)
}

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;
