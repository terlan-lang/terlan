use super::*;

/// Loads the checked-in inventory fixture.
fn fixture() -> Inventory {
    parse_inventory(include_str!(
        "../../../../docs/runtime/TVM_MULTICORE_INVARIANT_INVENTORY.json"
    ))
    .expect("inventory fixture")
}

/// Loads the checked-in frozen contract fixture.
fn contract() -> &'static str {
    include_str!("../../../../docs/runtime/TVM_MULTICORE_CONCURRENCY_CONTRACT.md")
}

/// Returns diagnostics for one mutated inventory.
fn diagnostics(inventory: &Inventory) -> Vec<String> {
    validate_inventory(inventory, contract())
}

/// Verifies the checked-in inventory is complete.
#[test]
fn multicore_inventory_accepts_complete_frozen_contract() {
    let inventory = fixture();
    assert!(diagnostics(&inventory).is_empty());
    let summary = summary(&inventory);
    assert_eq!(summary.entry_count, 18);
    assert_eq!(summary.domain_count, 16);
    assert_eq!(summary.classification_counts.len(), 4);
}

/// Verifies duplicate identities fail closed.
#[test]
fn multicore_inventory_rejects_duplicate_rows() {
    let mut inventory = fixture();
    inventory.entries[1].id = inventory.entries[0].id.clone();
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("duplicate or empty inventory id")));
}

/// Verifies missing and abbreviated revisions fail closed.
#[test]
fn multicore_inventory_rejects_missing_revision() {
    let mut inventory = fixture();
    inventory.upstream.revision = "f26c7e5".into();
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("full pinned")));
}

/// Verifies every retained invariant has a Terlan contract owner.
#[test]
fn multicore_inventory_rejects_unowned_retained_invariant() {
    let mut inventory = fixture();
    inventory.entries[0].contract_ids.clear();
    inventory.entries[0].test_identity = None;
    inventory.entries[0].planned_gate = None;
    let found = diagnostics(&inventory).join("\n");
    assert!(found.contains("has no contract owner"));
    assert!(found.contains("has no test identity"));
    assert!(found.contains("has no planned gate"));
}

/// Verifies unknown disposition classes fail closed.
#[test]
fn multicore_inventory_rejects_invalid_classification() {
    let mut inventory = fixture();
    inventory.entries[0].classification = "copy-erts-code".into();
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("invalid classification")));
}

/// Verifies an upstream checkout cannot become a product dependency.
#[test]
fn multicore_inventory_rejects_deleted_path_product_dependency() {
    let mut inventory = fixture();
    inventory.entries[0].product_dependency = true;
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("deleted OTP path as a product dependency")));
}

/// Verifies ERTS mechanics cannot be relabeled as Terlan semantics.
#[test]
fn multicore_inventory_rejects_erts_mechanics_as_semantics() {
    let mut inventory = fixture();
    let detail = inventory
        .entries
        .iter_mut()
        .find(|entry| entry.implementation_detail)
        .expect("implementation detail");
    detail.classification = "port-semantic-invariant".into();
    detail.contract_ids = vec!["MC-C01".into()];
    detail.test_identity = Some("copied_erts_layout".into());
    detail.planned_gate = Some("vm-actor-mutator-ownership-check".into());
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("mislabels ERTS mechanics")));
}

/// Verifies removed mechanics cannot retain product owners.
#[test]
fn multicore_inventory_rejects_removed_detail_with_gate_mapping() {
    let mut inventory = fixture();
    let detail = inventory
        .entries
        .iter_mut()
        .find(|entry| entry.implementation_detail)
        .expect("implementation detail");
    detail.planned_gate = Some("copied-erts-gate".into());
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("cannot own Terlan contract or gate")));
}

/// Verifies each required domain must have a classified row.
#[test]
fn multicore_inventory_rejects_missing_domain_coverage() {
    let mut inventory = fixture();
    inventory.entries.retain(|entry| entry.domain != "shutdown");
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("required domain `shutdown`")));
}

/// Verifies malformed upstream paths fail closed.
#[test]
fn multicore_inventory_rejects_product_relative_escape() {
    let mut inventory = fixture();
    inventory.entries[0].upstream_path = "../terlan/runtime.rs".into();
    assert!(diagnostics(&inventory)
        .iter()
        .any(|message| message.contains("invalid upstream path")));
}

/// Verifies every frozen contract clause remains present.
#[test]
fn multicore_inventory_rejects_missing_contract_clause() {
    let inventory = fixture();
    let damaged = contract().replace("## MC-C05 Park And Wake", "## Park And Wake");
    assert!(validate_inventory(&inventory, &damaged)
        .iter()
        .any(|message| message.contains("omits clause `MC-C05`")));
}
