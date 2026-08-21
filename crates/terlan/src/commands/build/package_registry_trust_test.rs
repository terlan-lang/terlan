use super::*;
use crate::package_registry::model::{PackageIndexVersion, ResourceSignature, SnapshotPackage};

fn fixture() -> (String, String, TrustPin, Vec<u8>, Vec<u8>, Vec<u8>) {
    let origin = "https://registry.example.test".to_string();
    let key_id = "root-1".to_string();
    let seed = STANDARD.encode([23_u8; 32]);
    let public_key = crate::runtime::native::ed25519::sign(&seed, "probe")
        .unwrap()
        .public_key_base64;
    let pin = TrustPin {
        schema: TRUST_PIN_SCHEMA.into(),
        origin: origin.clone(),
        key_id: key_id.clone(),
        algorithm: "ed25519".into(),
        public_key_base64: public_key.clone(),
    };
    let root = RootRecord {
        schema: "terlan-registry-root-v1".into(),
        version: 1,
        previous_version: None,
        threshold: 1,
        keys: vec![TrustKey {
            key_id: key_id.clone(),
            algorithm: "ed25519".into(),
            public_key_base64: public_key,
            roles: vec!["root".into(), "snapshot".into(), "package-index".into()],
        }],
        signed_digest: digest(b"placeholder"),
    };
    let root_bytes = envelope(&origin, "/repo/v1/root.json", &root, &key_id, &seed);

    let index = PackageIndexRecord {
        schema: "terlan-registry-package-index-v1".into(),
        name: "demo".into(),
        repository_url: "https://github.com/terlan-lang/demo".into(),
        versions: vec![PackageIndexVersion {
            version: "1.0.0".into(),
            archive: digest(b"archive"),
            metadata: digest(b"metadata"),
            documentation: None,
            built_with: "terlan-0.0.8".into(),
            requires_terlan: ">=0.0.8, <0.1.0".into(),
            published_sequence: 1,
            published_at: "2026-08-20T00:00:00.000000Z".into(),
            yanked: false,
            yank: None,
        }],
        latest_stable: Some("1.0.0".into()),
        signed_digest: digest(b"placeholder"),
    };
    let index_bytes = envelope(
        &origin,
        "/repo/v1/packages/demo.json",
        &index,
        &key_id,
        &seed,
    );
    let snapshot = SnapshotRecord {
        schema: "terlan-registry-snapshot-v1".into(),
        sequence: 1,
        root_version: 1,
        packages: vec![SnapshotPackage {
            name: "demo".into(),
            index: digest(&index_bytes),
        }],
        signed_digest: digest(b"placeholder"),
    };
    let snapshot_bytes = envelope(&origin, "/repo/v1/snapshot.json", &snapshot, &key_id, &seed);
    (origin, seed, pin, root_bytes, snapshot_bytes, index_bytes)
}

#[test]
fn verifies_origin_threshold_and_digest_before_using_resources() {
    let (origin, _seed, pin, root_bytes, snapshot_bytes, index_bytes) = fixture();
    let root = verify_root(&root_bytes, &origin, &pin, None).unwrap();
    let state = state_after_root(&origin, &root, None);
    let snapshot = verify_snapshot(&snapshot_bytes, &origin, &state).unwrap();
    let state = state_after_snapshot(&state, &snapshot);
    let expected = snapshot.value.packages.first().unwrap();
    let index = verify_package_index(
        &index_bytes,
        &origin,
        "/repo/v1/packages/demo.json",
        "demo",
        &expected.index,
        &state,
    )
    .unwrap();
    assert_eq!(index.value.name, "demo");

    let error = verify_root(&root_bytes, "https://mirror.example.test", &pin, None).unwrap_err();
    assert!(error.contains("registry_trust_pin_invalid"));

    let mut tampered = index_bytes;
    tampered[10] ^= 1;
    assert!(verify_package_index(
        &tampered,
        &origin,
        "/repo/v1/packages/demo.json",
        "demo",
        &expected.index,
        &state,
    )
    .is_err());
}

#[test]
fn rejects_snapshot_rollback_and_same_sequence_replacement() {
    let (origin, seed, pin, root_bytes, snapshot_bytes, _index_bytes) = fixture();
    let root = verify_root(&root_bytes, &origin, &pin, None).unwrap();
    let state = state_after_root(&origin, &root, None);
    let snapshot = verify_snapshot(&snapshot_bytes, &origin, &state).unwrap();
    let state = state_after_snapshot(&state, &snapshot);

    let replacement = SnapshotRecord {
        schema: "terlan-registry-snapshot-v1".into(),
        sequence: 1,
        root_version: 1,
        packages: vec![],
        signed_digest: digest(b"placeholder"),
    };
    let bytes = envelope(
        &origin,
        "/repo/v1/snapshot.json",
        &replacement,
        "root-1",
        &seed,
    );
    let error = verify_snapshot(&bytes, &origin, &state).unwrap_err();
    assert!(error.contains("registry_snapshot_replacement"));

    let mut future_state = state.clone();
    future_state.snapshot_sequence = Some(2);
    future_state.snapshot_envelope_sha256 = Some("f".repeat(64));
    let error = verify_snapshot(&snapshot_bytes, &origin, &future_state).unwrap_err();
    assert!(error.contains("registry_snapshot_rollback"));
}

#[test]
fn rejects_invalid_signatures_and_root_rollback() {
    let (origin, _seed, pin, root_bytes, _snapshot, _index) = fixture();
    let root = verify_root(&root_bytes, &origin, &pin, None).unwrap();
    let mut invalid: SignedResourceRecord = serde_json::from_slice(&root_bytes).unwrap();
    invalid.signatures[0].signature_base64 = STANDARD.encode([0_u8; 64]);
    let invalid = serde_json::to_vec(&invalid).unwrap();
    let error = verify_root(&invalid, &origin, &pin, None).unwrap_err();
    assert!(error.contains("registry_signature_threshold"));

    let mut future = state_after_root(&origin, &root, None);
    future.root.version = 2;
    let error = verify_root(&root_bytes, &origin, &pin, Some(&future)).unwrap_err();
    assert!(error.contains("registry_root_rollback"));
}

#[test]
fn rejects_duplicate_signatures_as_one_threshold_vote() {
    let (origin, seed, pin, _root_bytes, _snapshot, _index) = fixture();
    let public_key = crate::runtime::native::ed25519::sign(&seed, "probe")
        .unwrap()
        .public_key_base64;
    let root = RootRecord {
        schema: "terlan-registry-root-v1".into(),
        version: 1,
        previous_version: None,
        threshold: 2,
        keys: vec![
            TrustKey {
                key_id: "root-1".into(),
                algorithm: "ed25519".into(),
                public_key_base64: public_key.clone(),
                roles: vec!["root".into(), "snapshot".into(), "package-index".into()],
            },
            TrustKey {
                key_id: "root-2".into(),
                algorithm: "ed25519".into(),
                public_key_base64: public_key,
                roles: vec!["root".into(), "snapshot".into(), "package-index".into()],
            },
        ],
        signed_digest: digest(b"placeholder"),
    };
    let bytes = envelope(&origin, "/repo/v1/root.json", &root, "root-1", &seed);
    let mut value: SignedResourceRecord = serde_json::from_slice(&bytes).unwrap();
    value.signatures.push(value.signatures[0].clone());
    let bytes = serde_json::to_vec(&value).unwrap();
    let error = verify_root(&bytes, &origin, &pin, None).unwrap_err();
    assert!(error.contains("registry_signature_threshold"));
}

fn envelope<T: Serialize>(
    origin: &str,
    route: &str,
    value: &T,
    key_id: &str,
    seed: &str,
) -> Vec<u8> {
    let mut object = match serde_json::to_value(value).unwrap() {
        Value::Object(object) => object,
        _ => unreachable!(),
    };
    object.remove("signed_digest");
    let unsigned = serde_json::to_vec(&object).unwrap();
    object.insert(
        "signed_digest".into(),
        serde_json::to_value(digest(&unsigned)).unwrap(),
    );
    let payload = serde_json::to_vec(&object).unwrap();
    let payload_sha = sha256_hex(&payload);
    let payload_base64 = STANDARD.encode(&payload);
    let input =
        format!("{SIGNED_RESOURCE_SCHEMA}\n{origin}\n{route}\n{payload_sha}\n{payload_base64}");
    let signature = crate::runtime::native::ed25519::sign(seed, &input).unwrap();
    serde_json::to_vec(&SignedResourceRecord {
        schema: SIGNED_RESOURCE_SCHEMA.into(),
        origin: origin.into(),
        resource: route.into(),
        payload_base64,
        payload: Digest {
            algorithm: "sha256".into(),
            value: payload_sha,
        },
        signatures: vec![ResourceSignature {
            key_id: key_id.into(),
            algorithm: "ed25519".into(),
            signature_base64: signature.signature_base64,
        }],
    })
    .unwrap()
}

fn digest(bytes: &[u8]) -> Digest {
    Digest {
        algorithm: "sha256".into(),
        value: sha256_hex(bytes),
    }
}
