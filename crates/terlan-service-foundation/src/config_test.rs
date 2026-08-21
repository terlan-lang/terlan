use super::*;

#[test]
fn secret_debug_never_contains_resolved_material() {
    let reference = SecretRef::declared("registry.signing_key");
    assert_eq!(reference.name(), "registry.signing_key");
    assert_eq!(
        format!("{reference:?}"),
        "SecretRef { name: \"registry.signing_key\", value: \"[secret]\" }"
    );
}
