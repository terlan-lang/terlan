use super::{direct_std, source_test_support::assert_source_checks, value_package};

#[test]
fn compiled_sha256_source_tests_use_generic_package_bindings() {
    for name in [
        "sha256",
        "sha256_bytes",
        "sha256_framed",
        "sha256_domain_framed",
        "sha256_nul_separated",
    ] {
        let operation = format!("std.crypto.hash.{name}");
        assert!(!direct_std::supports(&operation));
        assert!(value_package::binding(&operation).is_some());
    }
    assert_source_checks(
        "hash_source",
        &include_str!("../../../../../../std/crypto/HashTest.terl")
            .replace("module std.crypto.HashTest.", "module hash_source."),
        &[
            "sha256_matches_empty_vector",
            "sha256_matches_ascii_vector",
            "domain_framed_hash_preserves_the_unframed_domain",
            "sha256_bytes_matches_binary_ascii_vector",
            "framed_sha256_distinguishes_field_boundaries",
            "nul_separated_sha256_preserves_release_identity_bytes",
        ],
    );
}

#[test]
fn compiled_ed25519_source_tests_use_generic_package_bindings() {
    let operation = "std.crypto.ed25519.verify";
    assert!(!direct_std::supports(operation));
    assert_eq!(value_package::binding(operation).unwrap().arity, 3);
    assert_source_checks(
        "crypto_source",
        &include_str!("../../../../../../std/crypto/Ed25519Test.terl")
            .replace("module std.crypto.Ed25519Test.", "module crypto_source."),
        &[
            "invalid_material_fails_closed",
            "rfc8032_empty_message_is_valid",
            "changed_message_is_invalid",
        ],
    );
}
