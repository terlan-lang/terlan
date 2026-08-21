use super::*;

#[test]
fn spdx_admission_uses_strict_identifiers_and_rejects_deprecated_ids() {
    validate_spdx_expression("Apache-2.0 OR MIT").expect("current SPDX expression");
    validate_spdx_expression("Apache-2.0 WITH LLVM-exception").expect("compatible SPDX exception");
    validate_spdx_expression("LicenseRef-Terlan-Commercial")
        .expect("valid local license reference");

    assert!(validate_spdx_expression("CustomLicense").is_err());
    assert!(validate_spdx_expression("MIT WITH Unknown-exception").is_err());
    assert!(validate_spdx_expression("wxWindows").is_err());
}

#[test]
fn public_https_admission_rejects_local_private_and_special_hosts() {
    assert!(valid_public_https_url(
        "https://github.com/terlan-lang/terlan"
    ));
    assert!(valid_public_https_url("https://8.8.8.8/terlan"));

    for rejected in [
        "http://github.com/terlan-lang/terlan",
        "https://localhost/terlan",
        "https://registry.local/terlan",
        "https://127.0.0.1/terlan",
        "https://10.0.0.1/terlan",
        "https://169.254.1.1/terlan",
        "https://100.64.0.1/terlan",
        "https://192.0.2.1/terlan",
        "https://[::1]/terlan",
        "https://[fe80::1]/terlan",
        "https://[fd00::1]/terlan",
        "https://user:secret@example.com/terlan",
        "https://example.com:8443/terlan",
    ] {
        assert!(!valid_public_https_url(rejected), "accepted {rejected}");
    }
    assert!(valid_registry_origin("http://127.0.0.1:8080/"));
    assert!(!valid_registry_origin("http://10.0.0.1:8080/"));
}
