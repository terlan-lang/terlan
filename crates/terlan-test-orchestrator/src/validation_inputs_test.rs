use super::*;

fn source_snapshot(global: &str, phase: &str) -> crate::source_inventory::SourceSnapshot {
    crate::source_inventory::SourceSnapshot {
        sha256: global.repeat(64),
        phase_receipt_sha256: phase.repeat(64),
        files: 1,
        bytes: 1,
        covered_paths: Default::default(),
    }
}

#[test]
fn semantic_admission_ignores_checkout_location_and_terminal_observations() {
    let first = semantic_admission(
        serde_json::json!({
            "before":{"path":"/first/checkout/target/tool", "identity_sha256":"abc"},
            "after":{"path":"changed"}, "verified":true,
            "external":"/opt/toolchain/bin/rustc",
        }),
        Path::new("/first/checkout"),
    );
    let second = semantic_admission(
        serde_json::json!({
            "before":{"path":"/second/checkout/target/tool", "identity_sha256":"abc"},
            "after":null, "verified":false,
            "external":"/opt/toolchain/bin/rustc",
        }),
        Path::new("/second/checkout"),
    );

    assert_eq!(first, second);
    assert_eq!(first["before"]["path"], "$CHECKOUT/target/tool");
    assert_eq!(first["external"], "/opt/toolchain/bin/rustc");
}

#[test]
fn semantic_admission_keeps_relative_names_and_content_identity() {
    let checkout = Path::new("/checkout");
    let first = semantic_admission(
        serde_json::json!({"source":"src/main.rs", "identity_sha256":"abc"}),
        checkout,
    );
    let changed = semantic_admission(
        serde_json::json!({"source":"src/main.rs", "identity_sha256":"def"}),
        checkout,
    );

    assert_eq!(first["source"], "src/main.rs");
    assert_ne!(first, changed);
}

#[test]
fn phase_receipts_bind_scoped_source_identity_not_unrelated_source_changes() {
    let mut inputs = ValidationInputs::default();
    inputs.source.before = Some(source_snapshot("a", "b"));
    let first = inputs.phase_receipt_identity().unwrap();
    inputs.source.before = Some(source_snapshot("c", "b"));
    assert_eq!(first, inputs.phase_receipt_identity().unwrap());
    inputs.source.before = Some(source_snapshot("c", "d"));
    assert_ne!(first, inputs.phase_receipt_identity().unwrap());
}
