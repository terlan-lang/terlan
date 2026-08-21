//! Trust-before-use verification for Terlan Registry repository resources.

use std::collections::BTreeSet;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::package_registry::model::{
    Digest, PackageIndexRecord, RootRecord, SignedResourceRecord, SnapshotRecord, TrustKey,
};

use super::package_registry_error::RegistryResult;

const SIGNED_RESOURCE_SCHEMA: &str = "terlan-registry-signed-resource-v1";
const TRUST_PIN_SCHEMA: &str = "terlan-registry-trust-pin-v1";
const TRUST_STATE_SCHEMA: &str = "terlan-registry-trust-state-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrustPin {
    pub(super) schema: String,
    pub(super) origin: String,
    pub(super) key_id: String,
    pub(super) algorithm: String,
    pub(super) public_key_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrustState {
    pub(super) schema: String,
    pub(super) origin: String,
    pub(super) root: RootRecord,
    pub(super) root_envelope_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) snapshot_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) snapshot_envelope_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct VerifiedResource<T> {
    pub(super) value: T,
    pub(super) envelope_sha256: String,
}

struct DecodedEnvelope {
    envelope: SignedResourceRecord,
    payload: Vec<u8>,
    envelope_sha256: String,
    signature_input: String,
}

pub(super) fn validate_pin(pin: &TrustPin, expected_origin: &str) -> RegistryResult<()> {
    if pin.schema != TRUST_PIN_SCHEMA
        || pin.origin != expected_origin
        || pin.algorithm != "ed25519"
        || !safe_key_id(&pin.key_id)
        || STANDARD
            .decode(&pin.public_key_base64)
            .map(|bytes| bytes.len() != 32)
            .unwrap_or(true)
    {
        return Err(
            "error[registry_trust_pin_invalid]: trust pin does not match the Registry origin"
                .into(),
        );
    }
    Ok(())
}

/// Verifies the current root or exactly one sequential root rotation.
pub(super) fn verify_root(
    bytes: &[u8],
    origin: &str,
    pin: &TrustPin,
    previous: Option<&TrustState>,
) -> RegistryResult<VerifiedResource<RootRecord>> {
    validate_pin(pin, origin)?;
    if let Some(previous) = previous {
        validate_state(previous, origin)?;
    }
    let decoded = decode_envelope(bytes, origin, "/repo/v1/root.json")?;
    match previous {
        None => verify_signatures(
            &decoded,
            std::slice::from_ref(&TrustKey {
                key_id: pin.key_id.clone(),
                algorithm: pin.algorithm.clone(),
                public_key_base64: pin.public_key_base64.clone(),
                roles: vec!["root".into()],
            }),
            "root",
            1,
        )?,
        Some(state) => verify_signatures(&decoded, &state.root.keys, "root", state.root.threshold)?,
    }

    let root: RootRecord = parse_verified_payload(&decoded, "terlan-registry-root-v1")?;
    validate_root_record(&root)?;
    match previous {
        None => {
            if root.version != 1
                || root.previous_version.is_some()
                || !root.keys.iter().any(|key| {
                    key.key_id == pin.key_id
                        && key.algorithm == pin.algorithm
                        && key.public_key_base64 == pin.public_key_base64
                        && key.roles.iter().any(|role| role == "root")
                })
            {
                return Err(
                    "error[registry_root_bootstrap]: root does not contain the pinned bootstrap key"
                        .into(),
                );
            }
            verify_signatures(&decoded, &root.keys, "root", root.threshold)?;
        }
        Some(state) if root.version < state.root.version => {
            return Err("error[registry_root_rollback]: Registry root version decreased".into())
        }
        Some(state) if root.version == state.root.version => {
            if decoded.envelope_sha256 != state.root_envelope_sha256 || root != state.root {
                return Err(
                    "error[registry_root_replacement]: current root version changed bytes".into(),
                );
            }
        }
        Some(state) => {
            if root.version != state.root.version + 1
                || root.previous_version != Some(state.root.version)
            {
                return Err(
                    "error[registry_root_rotation]: root rotation must be sequential".into(),
                );
            }
            verify_signatures(&decoded, &root.keys, "root", root.threshold)?;
        }
    }
    Ok(verified(decoded, root))
}

pub(super) fn state_after_root(
    origin: &str,
    root: &VerifiedResource<RootRecord>,
    previous: Option<&TrustState>,
) -> TrustState {
    TrustState {
        schema: TRUST_STATE_SCHEMA.into(),
        origin: origin.into(),
        root: root.value.clone(),
        root_envelope_sha256: root.envelope_sha256.clone(),
        snapshot_sequence: previous.and_then(|state| state.snapshot_sequence),
        snapshot_envelope_sha256: previous.and_then(|state| state.snapshot_envelope_sha256.clone()),
    }
}

pub(super) fn verify_snapshot(
    bytes: &[u8],
    origin: &str,
    state: &TrustState,
) -> RegistryResult<VerifiedResource<SnapshotRecord>> {
    validate_state(state, origin)?;
    let decoded = decode_envelope(bytes, origin, "/repo/v1/snapshot.json")?;
    verify_signatures(&decoded, &state.root.keys, "snapshot", state.root.threshold)?;
    let snapshot: SnapshotRecord = parse_verified_payload(&decoded, "terlan-registry-snapshot-v1")?;
    if snapshot.root_version != state.root.version || snapshot.sequence == 0 {
        return Err(
            "error[registry_snapshot_root]: snapshot does not name the trusted root".into(),
        );
    }
    if let Some(highest) = state.snapshot_sequence {
        if snapshot.sequence < highest {
            return Err(
                "error[registry_snapshot_rollback]: Registry snapshot sequence decreased".into(),
            );
        }
        if snapshot.sequence == highest
            && state.snapshot_envelope_sha256.as_deref() != Some(decoded.envelope_sha256.as_str())
        {
            return Err(
                "error[registry_snapshot_replacement]: current snapshot sequence changed bytes"
                    .into(),
            );
        }
    }
    Ok(verified(decoded, snapshot))
}

pub(super) fn state_after_snapshot(
    state: &TrustState,
    snapshot: &VerifiedResource<SnapshotRecord>,
) -> TrustState {
    let mut next = state.clone();
    next.snapshot_sequence = Some(snapshot.value.sequence);
    next.snapshot_envelope_sha256 = Some(snapshot.envelope_sha256.clone());
    next
}

pub(super) fn verify_package_index(
    bytes: &[u8],
    origin: &str,
    route: &str,
    expected_name: &str,
    expected_envelope_sha256: &Digest,
    state: &TrustState,
) -> RegistryResult<VerifiedResource<PackageIndexRecord>> {
    validate_state(state, origin)?;
    verify_digest("snapshot package index", expected_envelope_sha256, bytes)?;
    let decoded = decode_envelope(bytes, origin, route)?;
    verify_signatures(
        &decoded,
        &state.root.keys,
        "package-index",
        state.root.threshold,
    )?;
    let index: PackageIndexRecord =
        parse_verified_payload(&decoded, "terlan-registry-package-index-v1")?;
    if index.name != expected_name {
        return Err(
            "error[registry_index_identity_mismatch]: package index name differs from request"
                .into(),
        );
    }
    Ok(verified(decoded, index))
}

fn decode_envelope(bytes: &[u8], origin: &str, route: &str) -> RegistryResult<DecodedEnvelope> {
    let envelope: SignedResourceRecord = serde_json::from_slice(bytes)
        .map_err(|error| format!("error[registry_envelope_invalid]: {error}"))?;
    if envelope.schema != SIGNED_RESOURCE_SCHEMA
        || envelope.origin != origin
        || envelope.resource != route
    {
        return Err(
            "error[registry_envelope_identity]: signed resource origin or route differs".into(),
        );
    }
    if envelope.payload.algorithm != "sha256" || !is_sha256(&envelope.payload.value) {
        return Err("error[registry_envelope_digest]: payload digest is invalid".into());
    }
    let payload = STANDARD.decode(&envelope.payload_base64).map_err(|_| {
        "error[registry_envelope_payload]: payload is not canonical base64".to_string()
    })?;
    if sha256_hex(&payload) != envelope.payload.value {
        return Err("error[registry_envelope_digest]: payload digest differs".into());
    }
    let signature_input = format!(
        "{SIGNED_RESOURCE_SCHEMA}\n{}\n{}\n{}\n{}",
        envelope.origin, envelope.resource, envelope.payload.value, envelope.payload_base64
    );
    Ok(DecodedEnvelope {
        envelope,
        payload,
        envelope_sha256: sha256_hex(bytes),
        signature_input,
    })
}

fn verify_signatures(
    decoded: &DecodedEnvelope,
    keys: &[TrustKey],
    role: &str,
    threshold: u16,
) -> RegistryResult<()> {
    if threshold == 0 {
        return Err("error[registry_signature_threshold]: signature threshold is zero".into());
    }
    let mut accepted = BTreeSet::new();
    for signature in &decoded.envelope.signatures {
        if signature.algorithm != "ed25519" || accepted.contains(signature.key_id.as_str()) {
            continue;
        }
        let Some(key) = keys.iter().find(|key| {
            key.key_id == signature.key_id
                && key.algorithm == "ed25519"
                && key.roles.iter().any(|candidate| candidate == role)
        }) else {
            continue;
        };
        if crate::runtime::native::ed25519::verify(
            &key.public_key_base64,
            &decoded.signature_input,
            &signature.signature_base64,
        ) {
            accepted.insert(signature.key_id.as_str());
        }
    }
    if accepted.len() < usize::from(threshold) {
        return Err(format!(
            "error[registry_signature_threshold]: {role} requires {threshold} distinct valid signature(s), found {}",
            accepted.len()
        )
        .into());
    }
    Ok(())
}

fn parse_verified_payload<T: DeserializeOwned>(
    decoded: &DecodedEnvelope,
    expected_schema: &str,
) -> RegistryResult<T> {
    verify_internal_digest(&decoded.payload)?;
    let value: Value = serde_json::from_slice(&decoded.payload)
        .map_err(|error| format!("error[registry_payload_invalid]: {error}"))?;
    if value.get("schema").and_then(Value::as_str) != Some(expected_schema) {
        return Err(
            format!("error[registry_schema_unsupported]: expected `{expected_schema}`").into(),
        );
    }
    Ok(serde_json::from_value(value)
        .map_err(|error| format!("error[registry_payload_invalid]: {error}"))?)
}

fn verify_internal_digest(payload: &[u8]) -> RegistryResult<()> {
    let Value::Object(mut object) = serde_json::from_slice::<Value>(payload)
        .map_err(|error| format!("error[registry_payload_invalid]: {error}"))?
    else {
        return Err("error[registry_payload_invalid]: payload must be an object".into());
    };
    let digest: Digest =
        serde_json::from_value(object.remove("signed_digest").ok_or_else(|| {
            "error[registry_payload_digest]: signed_digest is absent".to_string()
        })?)
        .map_err(|_| "error[registry_payload_digest]: signed_digest is invalid".to_string())?;
    if digest.algorithm != "sha256" || !is_sha256(&digest.value) {
        return Err("error[registry_payload_digest]: signed_digest is invalid".into());
    }
    let unsigned = serde_json::to_vec(&object)
        .map_err(|error| format!("error[registry_payload_invalid]: {error}"))?;
    if sha256_hex(&unsigned) != digest.value {
        return Err("error[registry_payload_digest]: signed payload digest differs".into());
    }
    Ok(())
}

fn validate_root_record(root: &RootRecord) -> RegistryResult<()> {
    let mut key_ids = BTreeSet::new();
    if root.threshold == 0
        || usize::from(root.threshold) > root.keys.len()
        || root.keys.iter().any(|key| {
            !safe_key_id(&key.key_id)
                || key.algorithm != "ed25519"
                || !key_ids.insert(key.key_id.as_str())
                || STANDARD
                    .decode(&key.public_key_base64)
                    .map(|bytes| bytes.len() != 32)
                    .unwrap_or(true)
                || key.roles.is_empty()
                || key
                    .roles
                    .iter()
                    .any(|role| !matches!(role.as_str(), "root" | "snapshot" | "package-index"))
        })
    {
        return Err("error[registry_root_invalid]: trusted root policy is invalid".into());
    }
    for role in ["root", "snapshot", "package-index"] {
        if root
            .keys
            .iter()
            .filter(|key| key.roles.iter().any(|candidate| candidate == role))
            .count()
            < usize::from(root.threshold)
        {
            return Err(format!(
                "error[registry_root_invalid]: root cannot satisfy the {role} threshold"
            )
            .into());
        }
    }
    Ok(())
}

fn validate_state(state: &TrustState, origin: &str) -> RegistryResult<()> {
    if state.schema != TRUST_STATE_SCHEMA || state.origin != origin {
        return Err("error[registry_trust_state]: persisted trust state is invalid".into());
    }
    validate_root_record(&state.root)
}

fn verify_digest(label: &str, expected: &Digest, bytes: &[u8]) -> RegistryResult<()> {
    if expected.algorithm != "sha256"
        || !is_sha256(&expected.value)
        || sha256_hex(bytes) != expected.value
    {
        return Err(format!("error[registry_checksum_mismatch]: {label} digest differs").into());
    }
    Ok(())
}

fn verified<T>(decoded: DecodedEnvelope, value: T) -> VerifiedResource<T> {
    VerifiedResource {
        value,
        envelope_sha256: decoded.envelope_sha256,
    }
}

fn safe_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "package_registry_trust_test.rs"]
mod tests;
