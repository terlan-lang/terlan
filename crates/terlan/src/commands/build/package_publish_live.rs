//! Authenticated HTTP publication of one package sealed by Terlan tooling.

use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use url::{Host, Url};

use crate::package_registry::admission::{safe_identity_segment, validate_publish_request};
use crate::package_registry::model::PublishRequest;

use super::package_publish::PackageSealSummary;
use super::package_registry_error::RegistryResult;

const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LivePublishSummary {
    pub(super) package: String,
    pub(super) version: String,
    pub(super) publish_id: String,
    pub(super) snapshot_sequence: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateResponse {
    schema: String,
    publish_id: String,
    request_id: String,
    state: String,
}

struct UploadContext<'a> {
    agent: &'a ureq::Agent,
    origin: &'a str,
    publisher_key_id: &'a str,
    seed: &'a str,
    publish_id: &'a str,
}

pub(super) fn publish(
    sealed: PackageSealSummary,
    registry: &str,
    publisher_key_id: &str,
    signing_seed_file: &Path,
) -> RegistryResult<LivePublishSummary> {
    let origin = registry_origin(registry)?;
    if !safe_identity_segment(publisher_key_id) {
        return Err("error[registry_publish_key]: publisher key identity is invalid".into());
    }
    let seed = read_signing_seed(signing_seed_file)?;
    let mut request: PublishRequest = serde_json::from_slice(
        &fs::read(&sealed.request)
            .map_err(|error| format!("cannot read {}: {error}", sealed.request.display()))?,
    )
    .map_err(|error| format!("sealed publication request is invalid: {error}"))?;
    request.publisher_key_id = publisher_key_id.to_string();
    request.request_id = format!(
        "publish-{}",
        &request.package_version.provenance.value[..24]
    );
    validate_publish_request(&request).map_err(|error| error.to_string())?;
    let mut request_bytes =
        serde_json::to_vec_pretty(&request).map_err(|error| error.to_string())?;
    request_bytes.push(b'\n');
    fs::write(&sealed.request, &request_bytes)
        .map_err(|error| format!("cannot write {}: {error}", sealed.request.display()))?;
    let request_text = std::str::from_utf8(&request_bytes)
        .map_err(|_| "publication request is not UTF-8".to_string())?;
    let request_signature = sign(&seed, request_text)?;

    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_body(Some(Duration::from_secs(30)))
            .timeout_send_body(Some(Duration::from_secs(30)))
            .build(),
    );
    let create_url = format!(
        "{origin}/api/v1/packages/{}/releases",
        request.package_version.package.name
    );
    let (status, body) = send(
        &agent,
        "POST",
        &create_url,
        &[
            ("content-type", "application/json"),
            ("x-terlan-signature", &request_signature),
        ],
        &request_bytes,
    )?;
    if !matches!(status, 200 | 202) {
        return Err(http_failure("create publication", status, &body).into());
    }
    let created: CreateResponse = serde_json::from_slice(&body)
        .map_err(|error| format!("Registry create response is invalid: {error}"))?;
    if created.schema != "terlan-registry-publish-result-v1"
        || created.request_id != request.request_id
        || !safe_identity_segment(&created.publish_id)
    {
        return Err("error[registry_publish_response]: Registry create identity is invalid".into());
    }
    if created.state != "visible" {
        let upload_context = UploadContext {
            agent: &agent,
            origin: &origin,
            publisher_key_id,
            seed: &seed,
            publish_id: &created.publish_id,
        };
        upload(
            &upload_context,
            "archive",
            &request.package_version.archive.digest.value,
            &request.archive_upload,
            &sealed.archive,
        )?;
        if let (Some(documentation), Some(identity), Some(upload_name)) = (
            sealed.documentation.as_ref(),
            request.package_version.documentation.as_ref(),
            request.documentation_upload.as_deref(),
        ) {
            upload(
                &upload_context,
                "documentation",
                &identity.digest.value,
                upload_name,
                documentation,
            )?;
        }

        let request_sha256 = sha256_hex(&request_bytes);
        let payload = mutation_payload("finalize", &created.publish_id, &request_sha256, "");
        let signature = sign(&seed, &payload)?;
        let finalize_url = format!("{origin}/api/v1/publishes/{}/finalize", created.publish_id);
        let (status, body) = send(
            &agent,
            "POST",
            &finalize_url,
            &[
                ("x-terlan-publisher-key-id", publisher_key_id),
                ("x-terlan-signature", &signature),
            ],
            &[],
        )?;
        if !matches!(status, 200 | 201) {
            return Err(http_failure("finalize publication", status, &body).into());
        }
    }

    let status_url = format!("{origin}/api/v1/publishes/{}", created.publish_id);
    let (status, body) = send(&agent, "GET", &status_url, &[], &[])?;
    if status != 200 {
        return Err(http_failure("read publication status", status, &body).into());
    }
    let status_json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| format!("Registry status response is invalid: {error}"))?;
    if status_json.get("state").and_then(|value| value.as_str()) != Some("visible") {
        return Err("error[registry_publish_incomplete]: publication is not visible".into());
    }
    let snapshot_sequence = status_json
        .get("snapshot_sequence")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            "error[registry_publish_response]: visible status has no snapshot sequence".to_string()
        })?;
    Ok(LivePublishSummary {
        package: request.package_version.package.name,
        version: request.package_version.package.version,
        publish_id: created.publish_id,
        snapshot_sequence,
    })
}

fn upload(
    context: &UploadContext<'_>,
    purpose: &str,
    digest: &str,
    upload_name: &str,
    path: &Path,
) -> RegistryResult<()> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let payload = mutation_payload(purpose, context.publish_id, digest, upload_name);
    let signature = sign(context.seed, &payload)?;
    let url = format!(
        "{}/api/v1/publishes/{}/{purpose}",
        context.origin, context.publish_id
    );
    let (status, body) = send(
        context.agent,
        "PUT",
        &url,
        &[
            ("content-type", "application/zstd"),
            ("x-terlan-publisher-key-id", context.publisher_key_id),
            ("x-terlan-signature", &signature),
        ],
        &bytes,
    )?;
    if !matches!(status, 200 | 201) {
        return Err(http_failure(&format!("upload {purpose}"), status, &body).into());
    }
    Ok(())
}

fn send(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> RegistryResult<(u16, Vec<u8>)> {
    let result = match method {
        "GET" => {
            let mut request = agent.get(url);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.call()
        }
        "POST" => {
            let mut request = agent.post(url);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.send(body)
        }
        "PUT" => {
            let mut request = agent.put(url);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.send(body)
        }
        unsupported => {
            return Err(format!(
                "error[registry_transport]: unsupported Registry HTTP method `{unsupported}`"
            )
            .into())
        }
    };
    let mut response = match result {
        Ok(response) => response,
        Err(error) => {
            return Err(format!("error[registry_transport]: {method} failed: {error}").into())
        }
    };
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("error[registry_transport]: response read failed: {error}"))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("error[registry_transport]: Registry response exceeded 1 MiB".into());
    }
    Ok((status, bytes))
}

pub(super) fn registry_origin(value: &str) -> RegistryResult<String> {
    let url = Url::parse(value)
        .map_err(|_| "error[registry_url]: Registry URL is invalid".to_string())?;
    let local_http = url.scheme() == "http"
        && matches!(url.host(), Some(Host::Ipv4(address)) if address.is_loopback());
    if value.trim() != value
        || (!local_http && url.scheme() != "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(
            "error[registry_url]: Registry requires public HTTPS or loopback HTTP for local development"
                .into(),
        );
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn read_signing_seed(path: &Path) -> RegistryResult<String> {
    let seed = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read publisher signing seed {}: {error}",
            path.display()
        )
    })?;
    let seed = seed.trim().to_string();
    if crate::runtime::native::ed25519::sign(&seed, "terlan-registry-key-probe").is_none() {
        return Err(
            "error[registry_publish_key]: signing seed must be base64-encoded Ed25519 seed bytes"
                .into(),
        );
    }
    Ok(seed)
}

fn sign(seed: &str, payload: &str) -> RegistryResult<String> {
    crate::runtime::native::ed25519::sign(seed, payload)
        .map(|signed| signed.signature_base64)
        .ok_or_else(|| "error[registry_publish_key]: publisher signing failed".into())
}

fn mutation_payload(purpose: &str, publish_id: &str, digest: &str, upload_name: &str) -> String {
    format!("terlan-registry-mutation-v1\n{purpose}\n{publish_id}\n{digest}\n{upload_name}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn http_failure(action: &str, status: u16, body: &[u8]) -> String {
    let detail = String::from_utf8_lossy(body);
    format!(
        "error[registry_http]: {action} returned HTTP {status}: {}",
        detail.trim()
    )
}

#[cfg(test)]
#[path = "package_publish_live_test.rs"]
mod tests;
