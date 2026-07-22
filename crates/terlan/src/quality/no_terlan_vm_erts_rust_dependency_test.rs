use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Verifies default Make targets reject retired ERTS Rust gates.
///
/// Inputs:
/// - A Makefile target map where `check` calls an old ERTS Rust gate.
///
/// Output:
/// - Diagnostic naming the retired gate.
///
/// Transformation:
/// - Locks the rule that the old ERTS Rust migration tree cannot be part of
///   default release validation.
#[test]
fn default_targets_reject_retired_erts_rust_gate_calls() {
    let targets = parse_make_targets(
        "check:\n\t$(MAKE) terlan-vm-erts-rust-check\n\ntest:\n\ntest-release:\n\npublish-preflight:\n",
    );

    let diagnostics = validate_default_targets(&targets);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-vm-erts-rust-check")),
        "expected retired gate diagnostic: {diagnostics:?}"
    );
}

/// Verifies default Make targets reject direct ERTS Rust path usage.
///
/// Inputs:
/// - A Makefile target map where `test-release` touches the old tree.
///
/// Output:
/// - Diagnostic naming `terlan-vm/erts/rust`.
///
/// Transformation:
/// - Prevents accidental active dependencies even when the retired gate names
///   are not used.
#[test]
fn default_targets_reject_erts_rust_path_usage() {
    let targets = parse_make_targets(
        "check:\n\ntest:\n\ntest-release:\n\ttest -f terlan-vm/erts/rust/Cargo.toml\n\npublish-preflight:\n",
    );

    let diagnostics = validate_default_targets(&targets);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-vm/erts/rust")),
        "expected ERTS Rust path diagnostic: {diagnostics:?}"
    );
}

/// Verifies release artifact and publish targets are part of the quarantine.
///
/// Inputs:
/// - A Makefile target map where `release-artifact-current` calls an old ERTS
///   Rust gate.
///
/// Output:
/// - Diagnostic naming the retired gate.
///
/// Transformation:
/// - Prevents retired VM migration checks from re-entering through packaging
///   targets even when ordinary `check` and `test-release` stay clean.
#[test]
fn release_artifact_targets_reject_retired_erts_rust_gate_calls() {
    let targets = parse_make_targets(
        "check:\n\ntest:\n\ntest-release:\n\nrelease-artifact-current:\n\t$(MAKE) terlan-vm-rust-core-slice-check\n\nrelease-artifact-linux:\n\nrelease-artifact-smoke:\n\nrelease-artifact-installer-smoke:\n\npublish-preflight:\n\npublish:\n",
    );

    let diagnostics = validate_default_targets(&targets);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-vm-rust-core-slice-check")),
        "expected retired release artifact gate diagnostic: {diagnostics:?}"
    );
}

/// Verifies retired ERTS Rust targets cannot remain public Make commands.
///
/// Inputs:
/// - A Makefile target map that defines an old manual ERTS Rust target.
///
/// Output:
/// - Diagnostic naming the retired target definition.
///
/// Transformation:
/// - Prevents the quarantined tree from leaking back into the public release
///   command surface after it has been removed from default gates.
#[test]
fn retired_erts_rust_targets_must_not_be_defined() {
    let targets = parse_make_targets(
        "check:\n\nterlan-vm-rust-core-slice-check:\n\tcargo test --manifest-path terlan-vm/erts/rust/Cargo.toml\n",
    );

    let diagnostics = validate_retired_target_definitions_absent(&targets);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-vm-rust-core-slice-check")),
        "expected retired target definition diagnostic: {diagnostics:?}"
    );
}

/// Verifies quarantined targets cannot write Cargo output into the source tree.
///
/// Inputs:
/// - A retained target body with a source-tree Cargo target directory.
///
/// Output:
/// - Diagnostic rejecting `terlan-vm/erts/rust/target`.
///
/// Transformation:
/// - Keeps manual migration checks from mutating the retired runtime tree.
#[test]
fn quarantined_targets_reject_source_tree_cargo_target_output() {
    let targets = parse_make_targets(
        "terlan-vm-erts-rust-check:\n\tCARGO_TARGET_DIR=terlan-vm/erts/rust/target cargo test\n",
    );

    let diagnostics = validate_quarantined_targets(&targets);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan-vm/erts/rust/target")),
        "expected source-tree target diagnostic: {diagnostics:?}"
    );
}

/// Verifies quarantined Cargo targets require explicit temporary target dirs.
///
/// Inputs:
/// - A retained target body that invokes Cargo without a `/tmp` target dir.
///
/// Output:
/// - Diagnostic requiring a temporary Cargo target directory.
///
/// Transformation:
/// - Ensures retained migration targets stay opt-in and do not pollute the
///   source tree or default build cache assumptions.
#[test]
fn quarantined_cargo_targets_require_tmp_target_dir() {
    let targets = parse_make_targets("terlan-vm-erts-rust-check:\n\tcargo test\n");

    let diagnostics = validate_quarantined_targets(&targets);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("/tmp Cargo target directory")),
        "expected temp target diagnostic: {diagnostics:?}"
    );
}

/// Verifies quarantine documentation requires all contract terms.
///
/// Inputs:
/// - Incomplete quarantine documentation.
///
/// Output:
/// - Missing-term diagnostics.
///
/// Transformation:
/// - Keeps the migration-reference status explicit for future maintainers.
#[test]
fn quarantine_docs_reject_missing_required_terms() {
    let diagnostics = validate_quarantine_doc_text("reference only\n");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("quarantined migration reference material")),
        "expected missing term diagnostic: {diagnostics:?}"
    );
}

/// Verifies quarantine documentation rejects placeholder wording.
///
/// Inputs:
/// - Complete required terms plus placeholder planning text.
///
/// Output:
/// - Diagnostic naming the placeholder.
///
/// Transformation:
/// - Keeps retained ERTS Rust quarantine status executable rather than
///   future-planned.
#[test]
fn quarantine_docs_reject_placeholder_wording() {
    let text = format!(
        "{}\nTODO: decide whether this tree is still active later.",
        REQUIRED_DOC_TERMS.join("\n")
    );

    let diagnostics = validate_quarantine_doc_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder ERTS Rust quarantine text")),
        "expected placeholder diagnostic: {diagnostics:?}"
    );
}

/// Verifies retained inventory rejects unknown migrate-first crates.
///
/// Inputs:
/// - Inventory rows with allowed migrate-first crates plus one extra
///   migrate-first helper crate.
///
/// Output:
/// - Diagnostic naming the unexpected crate.
///
/// Transformation:
/// - Keeps migration evidence small and explicit instead of letting the old
///   ERTS Rust tree become a second runtime backlog.
#[test]
fn retained_inventory_rejects_unexpected_migrate_first_crates() {
    let rows = vec![
        inventory_row("terlan_vm", "vm-owned", "migrate-first"),
        inventory_row("terlan_erts_test_support", "test-support", "migrate-first"),
        inventory_row("epmd", "runtime-helper", "migrate-first"),
        inventory_row("run_erl", "runtime-helper", "migrate-first"),
    ];

    let diagnostics = validate_inventory_rows(&rows);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("run_erl")),
        "expected unexpected migrate-first diagnostic: {diagnostics:?}"
    );
}

/// Verifies retained inventory rejects malformed rows.
///
/// Inputs:
/// - A migration inventory with an invalid header and a row with missing
///   columns.
///
/// Output:
/// - Stable diagnostics for both malformed shapes.
///
/// Transformation:
/// - Prevents silent drift in the retained-tree inventory format.
#[test]
fn retained_inventory_rejects_malformed_rows() {
    let mut diagnostics = Vec::new();

    let rows = parse_migration_inventory(
        "crate,class,policy,evidence\nterlan_vm\tvm-owned\tmigrate-first\n",
        &mut diagnostics,
    );

    assert!(rows.is_empty(), "malformed row should not parse: {rows:?}");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must start")),
        "expected header diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("4 tab-separated columns")),
        "expected column diagnostic: {diagnostics:?}"
    );
}

/// Verifies migrate-first rows require golden-owned evidence.
///
/// Inputs:
/// - Inventory rows whose replacement evidence is missing or points back at
///   the retired ERTS Rust tree.
///
/// Output:
/// - Stable diagnostics for missing and non-golden evidence paths.
///
/// Transformation:
/// - Prevents retained first-migration rows from becoming open-ended markers
///   without an owned implementation path in the golden crate.
#[test]
fn retained_inventory_rejects_missing_migrate_first_evidence() {
    let root = temp_quality_root("retained_inventory_missing_evidence");
    fs::create_dir_all(root.join("crates/terlan/src/runtime/vm")).expect("create VM evidence");

    let rows = vec![
        inventory_row_with_evidence("terlan_vm", "vm-owned", "migrate-first", "-"),
        inventory_row_with_evidence(
            "terlan_erts_test_support",
            "test-support",
            "migrate-first",
            "crates/terlan/src/runtime/vm",
        ),
        inventory_row_with_evidence(
            "epmd",
            "runtime-helper",
            "migrate-first",
            "terlan-vm/erts/rust/epmd/src/lib.rs",
        ),
    ];

    let diagnostics = validate_migrate_first_golden_evidence(&root, &rows);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("terlan_vm")),
        "expected missing evidence diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must not point back")),
        "expected retired-tree evidence diagnostic: {diagnostics:?}"
    );
}

/// Verifies retained directories and inventory rows must match.
///
/// Inputs:
/// - Temporary retained-tree directories with one missing inventory entry.
///
/// Output:
/// - Diagnostic naming the retained directory missing from the inventory.
///
/// Transformation:
/// - Makes retained-tree deletion/shrinking auditable.
#[test]
fn retained_inventory_rejects_directory_drift() {
    let root = temp_quality_root("retained_inventory_directory_drift");
    let retained = root.join("terlan-vm/erts/rust");
    fs::create_dir_all(retained.join("terlan_vm")).expect("create terlan_vm dir");
    fs::create_dir_all(retained.join("epmd")).expect("create epmd dir");

    let rows = vec![inventory_row("terlan_vm", "vm-owned", "migrate-first")];

    let diagnostics = validate_inventory_matches_retained_dirs(&retained, &rows);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("epmd")),
        "expected missing epmd inventory diagnostic: {diagnostics:?}"
    );
}

/// Builds one inventory row for tests.
fn inventory_row(
    crate_name: &str,
    classification: &str,
    migration_policy: &str,
) -> MigrationInventoryRow {
    inventory_row_with_evidence(crate_name, classification, migration_policy, "-")
}

/// Builds one inventory row with explicit golden evidence for tests.
fn inventory_row_with_evidence(
    crate_name: &str,
    classification: &str,
    migration_policy: &str,
    golden_evidence: &str,
) -> MigrationInventoryRow {
    MigrationInventoryRow {
        crate_name: crate_name.to_string(),
        classification: classification.to_string(),
        migration_policy: migration_policy.to_string(),
        golden_evidence: golden_evidence.to_string(),
    }
}

/// Creates a unique temporary quality-test root.
fn temp_quality_root(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("terlan-quality-{name}-{unique}"));
    fs::create_dir_all(&root).expect("create temp quality root");
    root
}
