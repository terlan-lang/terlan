use super::*;

#[test]
fn verifies_exact_payload_and_fails_closed() {
    let payload = "terlan-registry-publish-request-v1";
    let signed = sign(&STANDARD.encode([7_u8; 32]), payload).expect("valid test seed");
    assert!(verify(
        &signed.public_key_base64,
        payload,
        &signed.signature_base64
    ));
    assert!(!verify(
        &signed.public_key_base64,
        "mutated",
        &signed.signature_base64
    ));
    assert!(!verify("invalid", payload, &signed.signature_base64));
}

#[test]
fn signs_exact_payload_from_strict_seed_material() {
    let seed = STANDARD.encode([7_u8; 32]);
    let signed = sign(&seed, "registry payload").expect("valid seed");
    assert!(verify(
        &signed.public_key_base64,
        "registry payload",
        &signed.signature_base64
    ));
    assert!(!verify(
        &signed.public_key_base64,
        "other payload",
        &signed.signature_base64
    ));
    assert!(sign("not-base64", "registry payload").is_none());
}
