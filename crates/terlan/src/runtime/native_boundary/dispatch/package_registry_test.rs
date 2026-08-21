use sha2::{Digest as _, Sha256};

use super::*;

fn valid_request_json() -> String {
    let artifact_path = "src/example/Example.terl";
    let artifact_digest = "a".repeat(64);
    let mut provenance = Sha256::new();
    provenance.update(b"terlan-package-provenance-v1");
    provenance.update([0]);
    provenance.update(artifact_path.as_bytes());
    provenance.update([0]);
    provenance.update(artifact_digest.as_bytes());
    let provenance = provenance
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    serde_json::json!({
        "schema": "terlan-registry-publish-request-v1",
        "package_version": {
            "schema": "terlan-registry-package-version-v1",
            "package": {"name": "example", "version": "1.0.0"},
            "repository_url": "https://github.com/terlan-lang/example",
            "description": "A valid publication request.",
            "license": "Apache-2.0",
            "links": [{
                "name": "github.com",
                "url": "https://github.com/terlan-lang/example"
            }],
            "archive": {
                "format": "tar.zst",
                "digest": {"algorithm": "sha256", "value": "b".repeat(64)},
                "compressed_bytes": 123,
                "unpacked_bytes": 456,
                "file_count": 1
            },
            "dependencies": [],
            "artifacts": [{
                "schema": "terlan-registry-artifact-v1",
                "kind": "source",
                "path": artifact_path,
                "digest": {"algorithm": "sha256", "value": artifact_digest},
                "bytes": 456,
                "executable": false
            }],
            "targets": ["terlan-vm"],
            "capabilities": [],
            "built_with": "terlan-0.0.8",
            "requires_terlan": ">=0.0.7, <0.1.0",
            "source_identity": {
                "kind": "artifact-set",
                "value": provenance,
                "verification": "registry-derived"
            },
            "provenance": {"algorithm": "sha256", "value": provenance},
            "public_api": {"algorithm": "sha256", "value": "c".repeat(64)}
        },
        "publisher_key_id": "publisher-2026",
        "request_id": "publish-0001",
        "archive_upload": "uploads/publish-0001.tar.zst",
        "limits": {
            "max_archive_bytes": 67108864,
            "max_unpacked_bytes": 268435456,
            "max_files": 4096,
            "max_path_bytes": 240,
            "symlinks": "reject"
        }
    })
    .to_string()
}

#[test]
fn valid_request_returns_only_registry_attempt_projection() {
    let result = dispatch(
        "std.package.registry.parse_publish_request",
        &[NativeBoundaryValue::Text(valid_request_json())],
    )
    .expect("valid publish request");
    let NativeBoundaryValue::Record { name, fields } = result else {
        unreachable!("publish parser must return a record")
    };
    assert_eq!(name, "PublishRequestProjection");
    assert_eq!(fields.len(), 19);
    assert!(fields.contains(&(
        "package_name".to_string(),
        NativeBoundaryValue::Text("example".to_string())
    )));
    assert!(fields.contains(&("archive_bytes".to_string(), NativeBoundaryValue::Int(123))));
    assert!(fields.contains(&(
        "documentation_sha256".to_string(),
        NativeBoundaryValue::Text(String::new())
    )));
}

#[test]
fn unknown_and_duplicate_fields_are_rejected_before_admission() {
    let mut unknown: serde_json::Value =
        serde_json::from_str(&valid_request_json()).expect("valid JSON fixture");
    unknown["untrusted"] = serde_json::Value::Bool(true);
    let error = parse_publish_request(&unknown.to_string()).expect_err("unknown field");
    assert_eq!(error.code(), "registry.publish_json");

    let duplicate = valid_request_json().replacen(
        "\"request_id\":",
        "\"request_id\":\"publish-forged\",\"request_id\":",
        1,
    );
    let error = parse_publish_request(&duplicate).expect_err("duplicate field");
    assert_eq!(error.code(), "registry.publish_json");
}

#[test]
fn shared_admission_rejects_semantically_unsafe_metadata() {
    let mut request: serde_json::Value =
        serde_json::from_str(&valid_request_json()).expect("valid JSON fixture");
    request["package_version"]["repository_url"] =
        serde_json::Value::String("http://internal.invalid/repository".to_string());
    let error = parse_publish_request(&request.to_string()).expect_err("unsafe repository URL");
    assert_eq!(error.code(), "registry.publish_admission");
    assert!(error.message().contains("registry_publish_target"));
}

#[test]
fn shared_admission_rejects_noncanonical_package_names_and_digests() {
    for name in [
        "Example",
        "1example",
        ".example",
        "example.",
        "example..core",
    ] {
        let mut request: serde_json::Value =
            serde_json::from_str(&valid_request_json()).expect("valid JSON fixture");
        request["package_version"]["package"]["name"] = serde_json::Value::String(name.to_string());
        let error =
            parse_publish_request(&request.to_string()).expect_err("noncanonical package name");
        assert_eq!(error.code(), "registry.publish_admission");
    }

    let mut request: serde_json::Value =
        serde_json::from_str(&valid_request_json()).expect("valid JSON fixture");
    request["package_version"]["archive"]["digest"]["value"] =
        serde_json::Value::String("A".repeat(64));
    let error = parse_publish_request(&request.to_string()).expect_err("uppercase SHA-256 digest");
    assert_eq!(error.code(), "registry.publish_admission");
}

#[test]
fn signed_yank_projection_is_strict_and_structured() {
    let yank = serde_json::json!({
        "schema": "terlan-registry-yank-v1",
        "package": {"name": "example", "version": "1.0.0"},
        "state": "yanked",
        "reason": "security",
        "message": "withdrawn after review",
        "replacement_package": "example_next",
        "publisher_key_id": "publisher-2026",
        "sequence": 2
    });
    let NativeBoundaryValue::Record { name, fields } =
        parse_yank_request(&yank.to_string()).expect("valid yank request")
    else {
        unreachable!("yank parser must return a record")
    };
    assert_eq!(name, "YankRequestProjection");
    assert!(fields.contains(&(
        "state".to_string(),
        NativeBoundaryValue::Text("yanked".to_string())
    )));
    assert!(fields.contains(&(
        "reason".to_string(),
        NativeBoundaryValue::Text("security".to_string())
    )));

    let mut invalid = yank;
    invalid["sequence"] = serde_json::json!(0);
    assert_eq!(
        parse_yank_request(&invalid.to_string())
            .expect_err("zero sequence")
            .code(),
        "registry.yank_admission"
    );
}
