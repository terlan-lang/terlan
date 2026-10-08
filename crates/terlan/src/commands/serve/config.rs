//! Host discovery and artifact I/O for package-owned HTTP configuration.

use std::fs;
use std::path::{Path, PathBuf};

use super::args::ServeArgs;
use super::manifest;
pub(super) use terlan_http_native::server_config::EffectiveServeConfig;
use terlan_http_native::server_config::{self, ServeManifest, ServeOverrides};
#[cfg(test)]
use terlan_http_native::server_config::{
    DEFAULT_POLL_MS, DEFAULT_SERVE_HOST, DEFAULT_SERVE_PORT, SERVE_CONFIG_SCHEMA,
};

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
    let config = manifest_path
        .as_deref()
        .filter(|path| path.is_file())
        .map(read_manifest)
        .transpose()?
        .unwrap_or_default();
    Ok(server_config::resolve(
        config,
        environment,
        cli_overrides(args),
        args.web_root.clone(),
        project_root.as_deref(),
        std::thread::available_parallelism()
            .map(|width| width.get() as u64)
            .unwrap_or(1),
    )
    .map_err(String::from)?)
}

fn read_manifest(path: &Path) -> super::ServeResult<ServeManifest> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read serve configuration {}: {error}",
            path.display()
        )
    })?;
    Ok(ServeManifest::parse(&source).map_err(|error| {
        format!(
            "cannot parse serve configuration {}: {error}",
            path.display()
        )
    })?)
}

fn cli_overrides(args: &ServeArgs) -> ServeOverrides {
    let overrides = &args.overrides;
    ServeOverrides {
        host: overrides.host.then(|| args.host.clone()),
        port: overrides.port.then_some(args.port),
        poll_ms: overrides.poll_ms.then_some(args.poll_ms),
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
    }
}

pub(super) fn write_effective_serve_config(
    config: &EffectiveServeConfig,
    web_root: &Path,
) -> super::ServeResult<PathBuf> {
    let path = super::runtime_artifact_directory(web_root)?.join("serve-effective-config.json");
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
