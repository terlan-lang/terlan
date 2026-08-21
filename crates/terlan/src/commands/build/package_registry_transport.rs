//! Bounded conditional HTTP and atomic verified storage for Registry clients.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::package_registry_error::RegistryResult;

const CACHE_REF_SCHEMA: &str = "terlan-registry-cache-ref-v1";

#[derive(Debug, Clone)]
pub(super) struct Download {
    pub(super) bytes: Vec<u8>,
    pub(super) etag: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CacheRef {
    schema: String,
    route: String,
    object_sha256: String,
    etag: String,
}

pub(super) struct RepositoryClient {
    origin: String,
    cache_root: PathBuf,
    offline: bool,
    agent: ureq::Agent,
}

impl RepositoryClient {
    pub(super) fn new(origin: String, cache_root: PathBuf, offline: bool) -> RegistryResult<Self> {
        super::package_publish_live::registry_origin(&origin)?;
        Ok(Self {
            origin,
            cache_root,
            offline,
            agent: ureq::Agent::new_with_config(
                ureq::Agent::config_builder()
                    .max_redirects(0)
                    .max_redirects_will_error(false)
                    .http_status_as_error(false)
                    .timeout_connect(Some(Duration::from_secs(10)))
                    .timeout_recv_body(Some(Duration::from_secs(30)))
                    .timeout_send_body(Some(Duration::from_secs(30)))
                    .build(),
            ),
        })
    }

    /// Reads a resource, using a verified cache entry for conditional or offline access.
    pub(super) fn get(&self, route: &str, max_bytes: u64) -> RegistryResult<Download> {
        validate_route(route)?;
        let cached = self.read_verified(route)?;
        if self.offline {
            return Ok(cached.ok_or_else(|| {
                format!(
                    "error[registry_offline_cache_miss]: `{route}` has no verified cached bytes"
                )
            })?);
        }
        let url = format!("{}{route}", self.origin);
        let mut last_transport_error = None;
        for attempt in 0..2 {
            let mut request = self.agent.get(&url).header("accept", "application/json");
            if let Some(cached) = &cached {
                request = request.header("if-none-match", &cached.etag);
            }
            let mut response = match request.call() {
                Ok(response) => response,
                Err(error) => {
                    last_transport_error = Some(error.to_string());
                    if attempt == 0 {
                        continue;
                    }
                    break;
                }
            };
            let status = response.status().as_u16();
            if status == 304 {
                return Ok(cached.ok_or_else(|| {
                    "error[registry_cache_invalid]: Registry returned 304 without verified bytes"
                        .to_string()
                })?);
            }
            if matches!(status, 502..=504) && attempt == 0 {
                continue;
            }
            if status != 200 {
                return Err(
                    format!("error[registry_http]: GET {route} returned HTTP {status}").into(),
                );
            }
            let etag = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| "error[registry_http]: Registry response has no ETag".to_string())?
                .to_string();
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .take(max_bytes + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| {
                    format!("error[registry_transport]: response read failed: {error}")
                })?;
            if bytes.len() as u64 > max_bytes {
                return Err(format!(
                    "error[registry_response_too_large]: `{route}` exceeded {max_bytes} bytes"
                )
                .into());
            }
            require_content_etag(&etag, &bytes)?;
            return Ok(Download { bytes, etag });
        }
        Err(format!(
            "error[registry_transport]: GET {route} failed after a safe retry: {}",
            last_transport_error.unwrap_or_else(|| "temporary Registry failure".into())
        )
        .into())
    }

    /// Makes bytes available to future conditional/offline reads only after callers verify them.
    pub(super) fn commit_verified(&self, route: &str, download: &Download) -> RegistryResult<()> {
        validate_route(route)?;
        require_content_etag(&download.etag, &download.bytes)?;
        let object_sha256 = sha256_hex(&download.bytes);
        let objects = self.cache_root.join("objects");
        let refs = self.cache_root.join("refs");
        fs::create_dir_all(&objects)
            .and_then(|_| fs::create_dir_all(&refs))
            .map_err(|error| format!("error[registry_cache_write]: {error}"))?;
        let object = objects.join(&object_sha256);
        if object.exists() {
            let actual = sha256_file(&object)?;
            if actual != object_sha256 {
                return Err("error[registry_cache_corrupt]: cached object digest differs".into());
            }
        } else {
            atomic_write(&object, &download.bytes)?;
        }
        let reference = CacheRef {
            schema: CACHE_REF_SCHEMA.into(),
            route: route.into(),
            object_sha256,
            etag: download.etag.clone(),
        };
        let bytes = serde_json::to_vec(&reference)
            .map_err(|error| format!("error[registry_cache_write]: {error}"))?;
        atomic_write(&refs.join(sha256_hex(route.as_bytes())), &bytes)
    }

    fn read_verified(&self, route: &str) -> RegistryResult<Option<Download>> {
        let reference_path = self
            .cache_root
            .join("refs")
            .join(sha256_hex(route.as_bytes()));
        if !reference_path.exists() {
            return Ok(None);
        }
        let reference: CacheRef = serde_json::from_slice(
            &fs::read(&reference_path)
                .map_err(|error| format!("error[registry_cache_read]: {error}"))?,
        )
        .map_err(|error| format!("error[registry_cache_corrupt]: {error}"))?;
        if reference.schema != CACHE_REF_SCHEMA
            || reference.route != route
            || !is_sha256(&reference.object_sha256)
        {
            return Err("error[registry_cache_corrupt]: cached reference is invalid".into());
        }
        let bytes = fs::read(
            self.cache_root
                .join("objects")
                .join(&reference.object_sha256),
        )
        .map_err(|error| format!("error[registry_cache_corrupt]: {error}"))?;
        if sha256_hex(&bytes) != reference.object_sha256 {
            return Err("error[registry_cache_corrupt]: cached object digest differs".into());
        }
        require_content_etag(&reference.etag, &bytes)?;
        Ok(Some(Download {
            bytes,
            etag: reference.etag,
        }))
    }
}

pub(super) fn atomic_json<T: Serialize>(path: &Path, value: &T) -> RegistryResult<()> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("error[registry_cache_write]: {error}"))?;
    atomic_write(path, &bytes)
}

pub(super) fn atomic_bytes(path: &Path, bytes: &[u8]) -> RegistryResult<()> {
    atomic_write(path, bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> RegistryResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "error[registry_cache_write]: path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("error[registry_cache_write]: {error}"))?;
    let temporary = parent.join(format!(
        ".tmp-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("error[registry_cache_write]: {error}"))?
            .as_nanos(),
        sha256_hex(path.as_os_str().as_encoded_bytes())
    ));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("error[registry_cache_write]: {error}"))?;
        use std::io::Write as _;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("error[registry_cache_write]: {error}"))?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|error| format!("error[registry_cache_write]: {error}"))?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path) -> RegistryResult<()> {
    Ok(fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("error[registry_cache_write]: {error}"))?)
}

fn validate_route(route: &str) -> RegistryResult<()> {
    if !route.starts_with('/')
        || route.starts_with("//")
        || route.contains("..")
        || route.contains('?')
        || route.contains('#')
        || route.contains('@')
        || !route.is_ascii()
    {
        return Err("error[registry_route_invalid]: Registry route is unsafe".into());
    }
    Ok(())
}

fn require_content_etag(etag: &str, bytes: &[u8]) -> RegistryResult<()> {
    let expected = format!("\"{}\"", sha256_hex(bytes));
    if etag != expected {
        return Err("error[registry_etag_mismatch]: ETag does not identify exact bytes".into());
    }
    Ok(())
}

fn sha256_file(path: &Path) -> RegistryResult<String> {
    Ok(fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| format!("error[registry_cache_read]: {error}"))?)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "package_registry_transport_test.rs"]
mod tests;
