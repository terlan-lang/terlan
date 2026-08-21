//! Deterministic JSON Schema documents for Registry protocol records.

use serde_json::{json, Map, Value};

use super::model::{
    MAX_ARCHIVE_BYTES, MAX_ARCHIVE_FILES, MAX_ARCHIVE_PATH_BYTES, MAX_UNPACKED_BYTES,
    PROTOCOL_VERSION,
};
use super::ProtocolDocument;

pub(super) fn schema_documents() -> Vec<ProtocolDocument> {
    vec![
        document(
            "publish-request.schema.json",
            "Terlan Registry Publish Request",
            "terlan-registry-publish-request-v1",
            properties([
                ("package_version", reference("package-version.schema.json")),
                ("publisher_key_id", non_empty_string()),
                ("request_id", non_empty_string()),
                ("archive_upload", safe_path()),
                ("documentation_upload", safe_path()),
                ("limits", archive_limits()),
            ]),
            &[
                "package_version",
                "publisher_key_id",
                "request_id",
                "archive_upload",
                "limits",
            ],
        ),
        document(
            "publish-result.schema.json",
            "Terlan Registry Publish Result",
            "terlan-registry-publish-result-v1",
            properties([
                ("publish_id", non_empty_string()),
                ("request_id", non_empty_string()),
                ("package", package_identity()),
                ("status", enum_string(&["accepted", "rejected"])),
                ("rejection_code", nullable_string()),
                ("snapshot", digest()),
            ]),
            &["publish_id", "request_id", "package", "status", "snapshot"],
        ),
        document(
            "package-version.schema.json",
            "Terlan Registry Package Version",
            "terlan-registry-package-version-v1",
            properties([
                ("package", package_identity()),
                ("repository_url", https_url()),
                ("description", non_empty_string()),
                ("license", non_empty_string()),
                ("links", array(package_link())),
                ("archive", archive_identity()),
                ("dependencies", array(reference("dependency.schema.json"))),
                ("artifacts", array(reference("artifact.schema.json"))),
                ("targets", unique_strings()),
                ("capabilities", unique_strings()),
                ("built_with", non_empty_string()),
                ("requires_terlan", non_empty_string()),
                ("source_identity", source_identity()),
                ("provenance", digest()),
                ("public_api", digest()),
                ("documentation", archive_identity()),
            ]),
            &[
                "package",
                "repository_url",
                "description",
                "license",
                "links",
                "archive",
                "dependencies",
                "artifacts",
                "targets",
                "capabilities",
                "built_with",
                "requires_terlan",
                "source_identity",
                "provenance",
                "public_api",
            ],
        ),
        document(
            "dependency.schema.json",
            "Terlan Registry Dependency",
            "terlan-registry-dependency-v1",
            properties([
                ("name", package_name()),
                (
                    "source",
                    enum_string(&["terlan-registry", "git", "path", "npm", "cargo"]),
                ),
                ("requirement", non_empty_string()),
                ("registry", non_empty_string()),
                ("optional", json!({"type": "boolean"})),
                ("target", nullable_string()),
                ("capabilities", unique_strings()),
                ("source_identity", non_empty_string()),
                ("integrity", digest()),
                ("options", unique_strings()),
            ]),
            &[
                "name",
                "source",
                "requirement",
                "registry",
                "optional",
                "capabilities",
                "options",
            ],
        ),
        document(
            "artifact.schema.json",
            "Terlan Registry Artifact",
            "terlan-registry-artifact-v1",
            properties([
                (
                    "kind",
                    enum_string(&[
                        "source",
                        "documentation",
                        "generated-binding",
                        "native",
                        "public-api",
                    ]),
                ),
                ("path", safe_path()),
                ("digest", digest()),
                ("bytes", json!({"type": "integer", "minimum": 0})),
                ("target", nullable_string()),
                ("executable", json!({"type": "boolean"})),
            ]),
            &["kind", "path", "digest", "bytes", "executable"],
        ),
        document(
            "yank.schema.json",
            "Terlan Registry Yank",
            "terlan-registry-yank-v1",
            properties([
                ("package", package_identity()),
                ("state", enum_string(&["yanked", "restored"])),
                (
                    "reason",
                    enum_string(&[
                        "security",
                        "invalid-metadata",
                        "deprecated",
                        "renamed",
                        "other",
                    ]),
                ),
                ("message", non_empty_string()),
                ("replacement_package", nullable_string()),
                ("publisher_key_id", non_empty_string()),
                ("sequence", positive_integer()),
            ]),
            &[
                "package",
                "state",
                "reason",
                "message",
                "publisher_key_id",
                "sequence",
            ],
        ),
        document(
            "root.schema.json",
            "Terlan Registry Root",
            "terlan-registry-root-v1",
            properties([
                ("version", positive_integer()),
                (
                    "previous_version",
                    json!({"type": ["integer", "null"], "minimum": 1}),
                ),
                ("threshold", json!({"type": "integer", "minimum": 1})),
                ("keys", array(trust_key())),
                ("signed_digest", digest()),
            ]),
            &[
                "version",
                "previous_version",
                "threshold",
                "keys",
                "signed_digest",
            ],
        ),
        document(
            "snapshot.schema.json",
            "Terlan Registry Snapshot",
            "terlan-registry-snapshot-v1",
            properties([
                ("sequence", positive_integer()),
                ("root_version", positive_integer()),
                ("packages", array(snapshot_package())),
                ("signed_digest", digest()),
            ]),
            &["sequence", "root_version", "packages", "signed_digest"],
        ),
        document(
            "package-index.schema.json",
            "Terlan Registry Package Index",
            "terlan-registry-package-index-v1",
            properties([
                ("name", package_name()),
                ("repository_url", https_url()),
                ("versions", array(index_version())),
                ("latest_stable", nullable_string()),
                ("signed_digest", digest()),
            ]),
            &["name", "repository_url", "versions", "signed_digest"],
        ),
        document(
            "signed-resource.schema.json",
            "Terlan Registry Signed Resource",
            "terlan-registry-signed-resource-v1",
            properties([
                ("origin", https_url()),
                (
                    "resource",
                    json!({"type": "string", "pattern": "^/repo/v1/", "minLength": 10}),
                ),
                ("payload_base64", non_empty_string()),
                ("payload", digest()),
                ("signatures", array(resource_signature())),
            ]),
            &[
                "origin",
                "resource",
                "payload_base64",
                "payload",
                "signatures",
            ],
        ),
    ]
}

fn https_url() -> Value {
    json!({"type": "string", "format": "uri", "pattern": "^https://", "minLength": 9})
}

fn package_link() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": {"type": "string", "minLength": 1},
            "url": https_url()
        },
        "required": ["name", "url"]
    })
}

fn document(
    file_name: &'static str,
    title: &str,
    schema: &str,
    mut record_properties: Map<String, Value>,
    record_required: &[&str],
) -> ProtocolDocument {
    record_properties.insert("schema".into(), json!({"const": schema}));
    let mut required = vec![Value::String("schema".into())];
    required.extend(
        record_required
            .iter()
            .map(|field| Value::String((*field).into())),
    );
    ProtocolDocument {
        file_name,
        value: json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": format!("https://registry.terlan.dev/schema/v1/{file_name}"),
            "title": title,
            "x-terlan-protocol": PROTOCOL_VERSION,
            "type": "object",
            "additionalProperties": false,
            "properties": record_properties,
            "required": required,
        }),
    }
}

fn properties<const N: usize>(entries: [(&str, Value); N]) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

fn reference(file_name: &str) -> Value {
    json!({"$ref": file_name})
}

fn array(items: Value) -> Value {
    json!({"type": "array", "items": items})
}

fn unique_strings() -> Value {
    json!({"type": "array", "items": {"type": "string", "minLength": 1}, "uniqueItems": true})
}

fn non_empty_string() -> Value {
    json!({"type": "string", "minLength": 1})
}

fn nullable_string() -> Value {
    json!({"type": ["string", "null"], "minLength": 1})
}

fn enum_string(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn positive_integer() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn package_name() -> Value {
    json!({
        "type": "string",
        "maxLength": 128,
        "pattern": "^[a-z](?:[a-z0-9_-]{0,126}[a-z0-9])?$"
    })
}

fn safe_path() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": MAX_ARCHIVE_PATH_BYTES,
        "pattern": "^(?!/)(?!.*(?:^|/)\\.\\.(?:/|$))(?!.*\\\\).+$"
    })
}

fn digest() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "algorithm": {"const": "sha256"},
            "value": {"type": "string", "pattern": "^[0-9a-f]{64}$"}
        },
        "required": ["algorithm", "value"]
    })
}

fn package_identity() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": package_name(),
            "version": {"type": "string", "pattern": "^[0-9]+\\.[0-9]+\\.[0-9]+(?:-[0-9A-Za-z.-]+)?$"}
        },
        "required": ["name", "version"]
    })
}

fn archive_limits() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "max_archive_bytes": {"const": MAX_ARCHIVE_BYTES},
            "max_unpacked_bytes": {"const": MAX_UNPACKED_BYTES},
            "max_files": {"const": MAX_ARCHIVE_FILES},
            "max_path_bytes": {"const": MAX_ARCHIVE_PATH_BYTES},
            "symlinks": {"const": "reject"}
        },
        "required": ["max_archive_bytes", "max_unpacked_bytes", "max_files", "max_path_bytes", "symlinks"]
    })
}

fn archive_identity() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "format": {"const": "tar.zst"},
            "digest": digest(),
            "compressed_bytes": {"type": "integer", "minimum": 1, "maximum": MAX_ARCHIVE_BYTES},
            "unpacked_bytes": {"type": "integer", "minimum": 1, "maximum": MAX_UNPACKED_BYTES},
            "file_count": {"type": "integer", "minimum": 1, "maximum": MAX_ARCHIVE_FILES}
        },
        "required": ["format", "digest", "compressed_bytes", "unpacked_bytes", "file_count"]
    })
}

fn source_identity() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "kind": {"enum": ["repository-commit", "artifact-set"]},
            "value": non_empty_string(),
            "verification": {"enum": ["maintainer-claimed", "registry-derived"]}
        },
        "required": ["kind", "value", "verification"]
    })
}

fn trust_key() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "key_id": non_empty_string(),
            "algorithm": {"const": "ed25519"},
            "public_key_base64": non_empty_string(),
            "roles": unique_strings()
        },
        "required": ["key_id", "algorithm", "public_key_base64", "roles"]
    })
}

fn resource_signature() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "key_id": non_empty_string(),
            "algorithm": {"const": "ed25519"},
            "signature_base64": non_empty_string()
        },
        "required": ["key_id", "algorithm", "signature_base64"]
    })
}

fn snapshot_package() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {"name": package_name(), "index": digest()},
        "required": ["name", "index"]
    })
}

fn index_version() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "version": non_empty_string(),
            "archive": digest(),
            "metadata": digest(),
            "documentation": digest(),
            "built_with": non_empty_string(),
            "requires_terlan": non_empty_string(),
            "published_sequence": positive_integer(),
            "published_at": non_empty_string(),
            "yanked": {"type": "boolean"},
            "yank": package_index_yank()
        },
        "required": ["version", "archive", "metadata", "built_with", "requires_terlan", "published_sequence", "published_at", "yanked"]
    })
}

fn package_index_yank() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "reason": {"enum": ["security", "invalid-metadata", "deprecated", "renamed", "other"]},
            "message": non_empty_string(),
            "replacement_package": nullable_string()
        },
        "required": ["reason", "message"]
    })
}
