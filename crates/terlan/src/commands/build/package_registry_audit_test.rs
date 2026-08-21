use super::*;

#[test]
fn audit_detects_cache_tampering_without_network_access() {
    let root = std::env::temp_dir().join(format!(
        "terlan-package-audit-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let bytes = b"sealed archive";
    let digest = sha256_hex(bytes);
    fs::write(
        root.join("terlan.lock"),
        format!(
            "version = 3\nresolver = \"terlan-registry-resolver-v2\"\n\n[[registry]]\nalias = \"demo\"\nname = \"demo\"\nversion = \"1.0.0\"\nregistry = \"https://registry.example\"\nsnapshot_sha256 = \"snapshot\"\nsource_identity = \"registry:https://registry.example/demo\"\narchive_sha256 = \"{digest}\"\nmetadata_sha256 = \"metadata\"\ncache_key = \"{digest}\"\nresolver = \"terlan-registry-resolver-v2\"\ntargets = []\ncapabilities = []\ndependencies = []\n"
        ),
    )
    .unwrap();
    let cache = root
        .join(".terlan/packages/registry")
        .join(&digest)
        .join("archive.tar.zst");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, bytes).unwrap();
    let clean = audit(&root).unwrap();
    assert!(clean.findings.is_empty());
    assert_eq!(clean.network_access, "disabled");
    fs::write(&cache, b"tampered").unwrap();
    let tampered = audit(&root).unwrap();
    assert!(tampered
        .findings
        .iter()
        .any(|finding| finding.code == "cache_poisoning"));
    fs::remove_dir_all(root).unwrap();
}
