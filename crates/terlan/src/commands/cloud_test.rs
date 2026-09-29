use super::*;

#[test]
fn cloud_origin_requires_https_or_loopback_http() {
    assert_eq!(
        cloud_origin("https://cloud.terlan.dev").unwrap(),
        "https://cloud.terlan.dev"
    );
    assert_eq!(
        cloud_origin("http://127.0.0.1:4100").unwrap(),
        "http://127.0.0.1:4100"
    );
    assert!(cloud_origin("http://cloud.example").is_err());
    assert!(cloud_origin("https://user@cloud.example").is_err());
}

#[test]
fn identities_and_log_bounds_fail_closed() {
    assert!(validate_user_id("usr_00000000000000000000000000000001").is_ok());
    assert!(validate_user_id("usr_0000000000000001").is_ok());
    assert!(validate_deployment_id("dep_00000000000000000000000000000001").is_ok());
    assert!(validate_deployment_id("dep_../escape").is_err());
    assert!(parse_logs_args(&["--lines".into(), "1001".into()]).is_err());
}

#[test]
fn deploy_defaults_to_source_build_and_accepts_admitted_release() {
    let source = parse_deploy_args(&[]).unwrap();
    assert!(source.release_id.is_none());
    let admitted = parse_deploy_args(&[
        "--project".into(),
        "terlan-registry".into(),
        "--release-id".into(),
        "rel_00000000000000000000000000000002".into(),
    ])
    .unwrap();
    assert_eq!(admitted.project.as_deref(), Some("terlan-registry"));
    assert_eq!(
        admitted.release_id.as_deref(),
        Some("rel_00000000000000000000000000000002")
    );
}

#[test]
fn artifact_result_requires_exact_validated_release_shape() {
    let release_id = "rel_0000000000000001";
    let valid = serde_json::json!({
        "schema": "terlan-cloud-artifact-result-v1",
        "state": "validated",
        "release_id": release_id,
        "sha256": "a".repeat(64),
    });
    assert!(require_validated_artifact(&valid, release_id).is_ok());
    let mut drift = valid.clone();
    drift["state"] = Value::String("uploaded".into());
    assert!(require_validated_artifact(&drift, release_id).is_err());
    drift = valid;
    drift["release_id"] = Value::String("rel_0000000000000002".into());
    assert!(require_validated_artifact(&drift, release_id).is_err());
    let mut bad_digest = serde_json::json!({
        "schema": "terlan-cloud-artifact-result-v1",
        "state": "validated",
        "release_id": release_id,
        "sha256": "z".repeat(64),
    });
    assert!(require_validated_artifact(&bad_digest, release_id).is_err());
    bad_digest["sha256"] = Value::String("a".repeat(63));
    assert!(require_validated_artifact(&bad_digest, release_id).is_err());
}

#[test]
fn release_source_metadata_is_url_and_path_safe() {
    assert!(validate_release_version("0.0.2-rc.1+build").is_ok());
    assert!(validate_release_version("0.0.2?redirect=https://bad.example").is_err());
    assert!(validate_repository("https://github.com/terlan/terlan-registry").is_ok());
    assert!(validate_repository("https://token@github.com/terlan/private").is_err());
    assert!(validate_repository("https://github.com/terlan/repo?token=secret").is_err());
}
