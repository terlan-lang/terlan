use super::*;

#[test]
fn registry_origin_allows_only_https_and_loopback_development_http() {
    assert_eq!(
        registry_origin("https://registry.terlan.dev/").unwrap(),
        "https://registry.terlan.dev"
    );
    assert_eq!(
        registry_origin("http://127.0.0.1:8080/").unwrap(),
        "http://127.0.0.1:8080"
    );
    assert!(registry_origin("http://registry.terlan.dev/").is_err());
    assert!(registry_origin("http://10.0.0.1:8080/").is_err());
    assert!(registry_origin("https://registry.terlan.dev/path").is_err());
}

#[test]
fn mutation_payload_matches_registry_wire_contract_exactly() {
    assert_eq!(
        mutation_payload("archive", "publish-1", "abc", "package.tar.zst"),
        "terlan-registry-mutation-v1\narchive\npublish-1\nabc\npackage.tar.zst"
    );
}
