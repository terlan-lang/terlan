//! Strict Terlan Registry protocol parsing at one owned-value boundary.

use std::path::Path;

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest as _, Sha256};

use crate::package_registry::admission::{
    canonical_package_name, safe_identity_segment, validate_archive_inventory,
    validate_publish_request,
};
use crate::package_registry::model::{
    PublishRequest, SourceIdentityKind, SourceIdentityVerification, YankReason, YankRecord,
    YankState,
};

use super::args::{expect_text, unknown_operation};
use super::{DispatchError, NativeBoundaryValue};

pub(super) fn dispatch(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    match operation {
        "std.package.registry.parse_publish_request" => {
            parse_publish_request(expect_text(operation, args, 0)?)
        }
        "std.package.registry.parse_yank_request" => {
            parse_yank_request(expect_text(operation, args, 0)?)
        }
        "std.package.registry.archive_inventory_valid" => archive_inventory_valid(
            expect_text(operation, args, 0)?,
            expect_text(operation, args, 1)?,
            expect_text(operation, args, 2)?,
        ),
        "std.package.registry.sign_resource" => sign_resource(
            expect_text(operation, args, 0)?,
            expect_text(operation, args, 1)?,
        ),
        "std.package.registry.canonical_payload" => {
            canonical_payload(expect_text(operation, args, 0)?)
        }
        "std.package.registry.root_payload" => root_payload(
            expect_text(operation, args, 0)?,
            expect_text(operation, args, 1)?,
        ),
        "std.package.registry.signing_seed_valid" => {
            signing_seed_valid(expect_text(operation, args, 0)?)
        }
        "std.package.registry.build_signed_resource" => build_signed_resource(
            expect_text(operation, args, 0)?,
            expect_text(operation, args, 1)?,
            expect_text(operation, args, 2)?,
            expect_text(operation, args, 3)?,
            expect_text(operation, args, 4)?,
        ),
        "std.package.registry.dependency_candidates_valid" => dependency_candidates_valid(
            expect_text(operation, args, 0)?,
            expect_text(operation, args, 1)?,
            expect_text(operation, args, 2)?,
        ),
        _ => Err(unknown_operation(operation)),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DependencyCandidates {
    name: String,
    versions: Vec<String>,
}

fn dependency_candidates_valid(
    request_text: &str,
    candidates_text: &str,
    expected_origin: &str,
) -> Result<NativeBoundaryValue, DispatchError> {
    let valid = serde_json::from_str::<PublishRequest>(request_text)
        .ok()
        .filter(|request| validate_publish_request(request).is_ok())
        .zip(serde_json::from_str::<Vec<DependencyCandidates>>(candidates_text).ok())
        .is_some_and(|(request, candidates)| {
            request
                .package_version
                .dependencies
                .iter()
                .all(|dependency| {
                    if dependency.source
                        != crate::package_registry::model::DependencySource::TerlanRegistry
                    {
                        return true;
                    }
                    dependency.registry == expected_origin
                        && candidates
                            .iter()
                            .find(|candidate| candidate.name == dependency.name)
                            .is_some_and(|candidate| {
                                candidate.versions.iter().any(|version| {
                                    crate::package_registry::requirement_matches(
                                        &dependency.requirement,
                                        version,
                                    )
                                    .unwrap_or(false)
                                })
                            })
                })
        });
    Ok(NativeBoundaryValue::Bool(valid))
}

fn sign_resource(seed_base64: &str, payload: &str) -> Result<NativeBoundaryValue, DispatchError> {
    let signed = crate::runtime::native::ed25519::sign(seed_base64, payload);
    let (public_key_base64, signature_base64) = signed
        .map(|value| (value.public_key_base64, value.signature_base64))
        .unwrap_or_default();
    Ok(NativeBoundaryValue::Record {
        name: "RegistrySignature".to_string(),
        fields: vec![
            text_field("public_key_base64", &public_key_base64),
            text_field("signature_base64", &signature_base64),
        ],
    })
}

fn canonical_payload(payload: &str) -> Result<NativeBoundaryValue, DispatchError> {
    Ok(NativeBoundaryValue::Text(
        canonical_payload_text(payload).unwrap_or_default(),
    ))
}

fn canonical_payload_text(payload: &str) -> Option<String> {
    let Value::Object(mut object) = serde_json::from_str::<Value>(payload).ok()? else {
        return None;
    };
    object.remove("signed_digest");
    let unsigned = serde_json::to_string(&object).ok()?;
    object.insert(
        "signed_digest".to_string(),
        json!({"algorithm": "sha256", "value": sha256_hex(unsigned.as_bytes())}),
    );
    serde_json::to_string(&object).ok()
}

fn root_payload(
    key_id: &str,
    public_key_base64: &str,
) -> Result<NativeBoundaryValue, DispatchError> {
    if key_id.is_empty() || public_key_base64.is_empty() {
        return Ok(NativeBoundaryValue::Text(String::new()));
    }
    let payload = json!({
        "schema": "terlan-registry-root-v1",
        "version": 1,
        "previous_version": null,
        "threshold": 1,
        "keys": [{
            "key_id": key_id,
            "algorithm": "ed25519",
            "public_key_base64": public_key_base64,
            "roles": ["root", "snapshot", "package-index"]
        }]
    });
    let payload = serde_json::to_string(&payload).unwrap_or_default();
    canonical_payload(&payload)
}

fn signing_seed_valid(seed_base64: &str) -> Result<NativeBoundaryValue, DispatchError> {
    let valid = base64::engine::general_purpose::STANDARD
        .decode(seed_base64)
        .is_ok_and(|seed| seed.len() == 32);
    Ok(NativeBoundaryValue::Bool(valid))
}

fn build_signed_resource(
    origin: &str,
    route: &str,
    payload: &str,
    key_id: &str,
    seed_base64: &str,
) -> Result<NativeBoundaryValue, DispatchError> {
    const FALLBACK_SEED: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let fields_valid =
        !origin.is_empty() && !route.is_empty() && !payload.is_empty() && !key_id.is_empty();
    let effective_origin = if origin.is_empty() {
        "invalid-origin"
    } else {
        origin
    };
    let effective_route = if route.is_empty() { "/invalid" } else { route };
    let effective_payload = if payload.is_empty() { "{}" } else { payload };
    let effective_key_id = if key_id.is_empty() {
        "invalid-key"
    } else {
        key_id
    };

    let payload_sha256 = sha256_hex(effective_payload.as_bytes());
    let payload_base64 =
        base64::engine::general_purpose::STANDARD.encode(effective_payload.as_bytes());
    let signature_input = format!(
        "terlan-registry-signed-resource-v1\n{effective_origin}\n{effective_route}\n{payload_sha256}\n{payload_base64}"
    );
    let requested_signature = crate::runtime::native::ed25519::sign(seed_base64, &signature_input);
    let signing_valid = requested_signature.is_some();
    let Some(signature) = requested_signature
        .or_else(|| crate::runtime::native::ed25519::sign(FALLBACK_SEED, &signature_input))
    else {
        return Err(DispatchError::new(
            "dispatch.registry_fallback_signing",
            "the embedded Registry fallback signing seed is invalid",
            0,
        ));
    };

    let envelope_sha256 = if fields_valid && signing_valid {
        None
    } else {
        Some("0000000000000000000000000000000000000000000000000000000000000000")
    };

    let envelope = Value::Object(Map::from_iter([
        (
            "schema".to_string(),
            Value::String("terlan-registry-signed-resource-v1".to_string()),
        ),
        (
            "origin".to_string(),
            Value::String(effective_origin.to_string()),
        ),
        (
            "resource".to_string(),
            Value::String(effective_route.to_string()),
        ),
        ("payload_base64".to_string(), Value::String(payload_base64)),
        (
            "payload".to_string(),
            json!({"algorithm": "sha256", "value": payload_sha256}),
        ),
        (
            "signatures".to_string(),
            json!([{
                "key_id": effective_key_id,
                "algorithm": "ed25519",
                "signature_base64": signature.signature_base64
            }]),
        ),
    ]));
    let body = serde_json::to_string(&envelope).unwrap_or_default();
    let envelope_sha256 = envelope_sha256
        .map(str::to_string)
        .unwrap_or_else(|| sha256_hex(body.as_bytes()));
    Ok(signed_resource_value(
        &body,
        &envelope_sha256,
        &payload_sha256,
        &signature.public_key_base64,
    ))
}

fn signed_resource_value(
    body: &str,
    sha256: &str,
    payload_sha256: &str,
    public_key_base64: &str,
) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "SignedResource".to_string(),
        fields: vec![
            text_field("body", body),
            text_field("sha256", sha256),
            text_field("payload_sha256", payload_sha256),
            text_field("public_key_base64", public_key_base64),
        ],
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn archive_inventory_valid(
    request_text: &str,
    archive_path: &str,
    staging_path: &str,
) -> Result<NativeBoundaryValue, DispatchError> {
    let valid = serde_json::from_str::<PublishRequest>(request_text)
        .ok()
        .filter(|request| validate_publish_request(request).is_ok())
        .is_some_and(|request| {
            validate_archive_inventory(Path::new(archive_path), &request, Path::new(staging_path))
                .is_ok()
        });
    Ok(NativeBoundaryValue::Bool(valid))
}

/// Strictly decodes and admits one signed Registry publication request.
pub fn parse_publish_request(text: &str) -> Result<NativeBoundaryValue, DispatchError> {
    let request: PublishRequest = serde_json::from_str(text).map_err(|error| {
        DispatchError::new(
            "registry.publish_json",
            format!("publish request JSON is invalid: {error}"),
            error.column(),
        )
    })?;
    validate_publish_request(&request)
        .map_err(|error| DispatchError::new("registry.publish_admission", error.to_string(), 0))?;

    let package = &request.package_version;
    let documentation = package.documentation.as_ref();
    let source_identity_kind = match package.source_identity.kind {
        SourceIdentityKind::RepositoryCommit => "repository-commit",
        SourceIdentityKind::ArtifactSet => "artifact-set",
    };
    let source_identity_verification = match package.source_identity.verification {
        SourceIdentityVerification::MaintainerClaimed => "maintainer-claimed",
        SourceIdentityVerification::RegistryDerived => "registry-derived",
    };

    Ok(NativeBoundaryValue::Record {
        name: "PublishRequestProjection".to_string(),
        fields: vec![
            text_field("publisher_key_id", &request.publisher_key_id),
            text_field("request_id", &request.request_id),
            text_field("package_name", &package.package.name),
            text_field("version", &package.package.version),
            text_field("repository_url", &package.repository_url),
            text_field("description", &package.description),
            text_field("license_expression", &package.license),
            text_field("archive_sha256", &package.archive.digest.value),
            int_field("archive_bytes", package.archive.compressed_bytes)?,
            text_field(
                "documentation_sha256",
                documentation
                    .map(|identity| identity.digest.value.as_str())
                    .unwrap_or(""),
            ),
            int_field(
                "documentation_bytes",
                documentation
                    .map(|identity| identity.compressed_bytes)
                    .unwrap_or(0),
            )?,
            text_field("built_with", &package.built_with),
            text_field("requires_terlan", &package.requires_terlan),
            text_field("source_identity_kind", source_identity_kind),
            text_field("source_identity", &package.source_identity.value),
            text_field("source_identity_verification", source_identity_verification),
            text_field("provenance_sha256", &package.provenance.value),
            text_field("archive_upload", &request.archive_upload),
            text_field(
                "documentation_upload",
                request.documentation_upload.as_deref().unwrap_or(""),
            ),
        ],
    })
}

/// Strictly decodes one signed yank or restore request.
pub fn parse_yank_request(text: &str) -> Result<NativeBoundaryValue, DispatchError> {
    let request: YankRecord = serde_json::from_str(text).map_err(|error| {
        DispatchError::new(
            "registry.yank_json",
            format!("yank request JSON is invalid: {error}"),
            error.column(),
        )
    })?;
    if request.schema != "terlan-registry-yank-v1"
        || !canonical_package_name(&request.package.name)
        || crate::package_registry::canonical_version(&request.package.version).is_err()
        || !safe_identity_segment(&request.publisher_key_id)
        || request.sequence == 0
        || request.message.trim() != request.message
        || request.message.is_empty()
        || request.message.len() > 500
        || request
            .replacement_package
            .as_deref()
            .is_some_and(|replacement| {
                !canonical_package_name(replacement) || replacement == request.package.name
            })
    {
        return Err(DispatchError::new(
            "registry.yank_admission",
            "yank request metadata is invalid",
            0,
        ));
    }
    let state = match request.state {
        YankState::Yanked => "yanked",
        YankState::Restored => "restored",
    };
    let reason = match request.reason {
        YankReason::Security => "security",
        YankReason::InvalidMetadata => "invalid-metadata",
        YankReason::Deprecated => "deprecated",
        YankReason::Renamed => "renamed",
        YankReason::Other => "other",
    };
    Ok(NativeBoundaryValue::Record {
        name: "YankRequestProjection".to_string(),
        fields: vec![
            text_field("publisher_key_id", &request.publisher_key_id),
            text_field("package_name", &request.package.name),
            text_field("version", &request.package.version),
            text_field("state", state),
            text_field("reason", reason),
            text_field("message", &request.message),
            text_field(
                "replacement_package",
                request.replacement_package.as_deref().unwrap_or(""),
            ),
            int_field("sequence", request.sequence)?,
        ],
    })
}

fn text_field(name: &str, value: &str) -> (String, NativeBoundaryValue) {
    (
        name.to_string(),
        NativeBoundaryValue::Text(value.to_string()),
    )
}

fn int_field(name: &str, value: u64) -> Result<(String, NativeBoundaryValue), DispatchError> {
    let value = i64::try_from(value).map_err(|_| {
        DispatchError::new(
            "registry.publish_integer",
            format!("publish request field `{name}` exceeds Terlan Int"),
            0,
        )
    })?;
    Ok((name.to_string(), NativeBoundaryValue::Int(value)))
}

#[cfg(test)]
#[path = "package_registry_test.rs"]
mod tests;
