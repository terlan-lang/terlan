use super::*;

#[test]
fn verified_cache_is_content_addressed_and_offline_explicit() {
    let root = temp("verified");
    let route = "/repo/v1/snapshot.json";
    let bytes = br#"{"schema":"test"}"#.to_vec();
    let download = Download {
        etag: format!("\"{}\"", sha256_hex(&bytes)),
        bytes,
    };
    let writer =
        RepositoryClient::new("https://registry.example.test".into(), root.clone(), false).unwrap();
    writer.commit_verified(route, &download).unwrap();
    let offline =
        RepositoryClient::new("https://registry.example.test".into(), root.clone(), true).unwrap();
    assert_eq!(offline.get(route, 1024).unwrap().bytes, download.bytes);
    let error = offline.get("/repo/v1/root.json", 1024).unwrap_err();
    assert!(error.contains("registry_offline_cache_miss"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cache_rejects_tampering_and_non_content_etags() {
    let root = temp("tamper");
    let route = "/repo/v1/snapshot.json";
    let bytes = b"trusted".to_vec();
    let client =
        RepositoryClient::new("https://registry.example.test".into(), root.clone(), false).unwrap();
    let invalid = Download {
        bytes: bytes.clone(),
        etag: "\"wrong\"".into(),
    };
    assert!(client.commit_verified(route, &invalid).is_err());
    let valid = Download {
        etag: format!("\"{}\"", sha256_hex(&bytes)),
        bytes,
    };
    client.commit_verified(route, &valid).unwrap();
    let reference: CacheRef = serde_json::from_slice(
        &fs::read(root.join("refs").join(sha256_hex(route.as_bytes()))).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("objects").join(reference.object_sha256),
        b"tampered",
    )
    .unwrap();
    let offline =
        RepositoryClient::new("https://registry.example.test".into(), root.clone(), true).unwrap();
    assert!(offline
        .get(route, 1024)
        .unwrap_err()
        .contains("cache_corrupt"));
    fs::remove_dir_all(root).unwrap();
}

fn temp(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-registry-transport-{label}-{}-{nonce}",
        std::process::id()
    ))
}
