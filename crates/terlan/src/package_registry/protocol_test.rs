use std::fs;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::fixtures::fixture_documents;
use super::model::*;
use super::output::write_protocol_bundle;
use super::schema::schema_documents;

#[test]
fn protocol_has_ten_strict_schemas_and_typed_fixtures() {
    let schemas = schema_documents();
    let fixtures = fixture_documents().expect("fixtures");
    assert_eq!(schemas.len(), 10);
    assert_eq!(fixtures.len(), 10);
    for document in schemas {
        assert_eq!(document.value["additionalProperties"], false);
        assert_eq!(document.value["x-terlan-protocol"], PROTOCOL_VERSION);
        assert!(document.value["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "schema"));
    }
    typed::<PublishRequest>(&fixtures, "publish-request.json");
    typed::<PublishResult>(&fixtures, "publish-result.json");
    typed::<PackageVersionRecord>(&fixtures, "package-version.json");
    typed::<DependencyRecord>(&fixtures, "dependency.json");
    typed::<ArtifactRecord>(&fixtures, "artifact.json");
    typed::<YankRecord>(&fixtures, "yank.json");
    typed::<RootRecord>(&fixtures, "root.json");
    typed::<SnapshotRecord>(&fixtures, "snapshot.json");
    typed::<PackageIndexRecord>(&fixtures, "package-index.json");
    typed::<SignedResourceRecord>(&fixtures, "signed-resource.json");
}

#[test]
fn protocol_bundle_is_deterministic_and_records_archive_policy() {
    let first = temporary_directory("first");
    let second = temporary_directory("second");
    write_protocol_bundle(&first).unwrap();
    write_protocol_bundle(&second).unwrap();
    compare_tree(&first, &second, &first);

    let request: PublishRequest =
        serde_json::from_slice(&fs::read(first.join("fixtures/publish-request.json")).unwrap())
            .unwrap();
    assert_eq!(request.limits.max_archive_bytes, MAX_ARCHIVE_BYTES);
    assert_eq!(request.limits.max_unpacked_bytes, MAX_UNPACKED_BYTES);
    assert_eq!(request.limits.max_files, MAX_ARCHIVE_FILES);
    assert_eq!(request.limits.max_path_bytes, MAX_ARCHIVE_PATH_BYTES);
    assert_eq!(request.limits.symlinks, SymlinkPolicy::Reject);

    let manifest: Value =
        serde_json::from_slice(&fs::read(first.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["files"].as_array().unwrap().len(), 20);
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
}

#[test]
fn public_records_reject_unknown_fields() {
    let mut value = fixture_documents().unwrap().remove(3).value;
    value["unknown"] = Value::Bool(true);
    assert!(serde_json::from_value::<DependencyRecord>(value).is_err());
}

fn typed<T: DeserializeOwned>(fixtures: &[super::ProtocolDocument], name: &str) {
    let value = fixtures
        .iter()
        .find(|fixture| fixture.file_name == name)
        .unwrap()
        .value
        .clone();
    serde_json::from_value::<T>(value).expect("fixture must match its public Rust model");
}

fn compare_tree(first: &std::path::Path, second: &std::path::Path, current: &std::path::Path) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let relative = entry.path().strip_prefix(first).unwrap().to_path_buf();
        if entry.file_type().unwrap().is_dir() {
            compare_tree(first, second, &entry.path());
        } else {
            assert_eq!(
                fs::read(entry.path()).unwrap(),
                fs::read(second.join(relative)).unwrap()
            );
        }
    }
}

fn temporary_directory(label: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-registry-protocol-{label}-{}-{nonce}",
        std::process::id()
    ))
}
