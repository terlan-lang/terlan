use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

const INVENTORY_HEADER_ROW: &str = "path\tclassification\towner\tnotes\n";

/// Verifies the VM HashMap inventory parser accepts valid rows.
///
/// Inputs:
/// - A minimal TSV inventory with one classified VM runtime source file.
///
/// Output:
/// - One parsed inventory row.
///
/// Transformation:
/// - Locks the checked inventory shape used by the repository gate.
#[test]
fn vm_hashmap_inventory_parser_accepts_valid_rows() {
    let rows = parse_vm_hashmap_inventory(&format!(
        "{INVENTORY_HEADER_ROW}crates/terlan/src/runtime/vm/tcp.rs\ttransport-registry\tvm-io\tlookup by listener and stream handle\n"
    ))
    .expect("parse inventory");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].classification, "transport-registry");
}

/// Verifies unsupported VM HashMap classifications are rejected.
///
/// Inputs:
/// - One inventory row with a made-up classification.
///
/// Output:
/// - Diagnostic naming the unsupported classification.
///
/// Transformation:
/// - Prevents unchecked categories from weakening the deterministic VM
///   HashMap contract.
#[test]
fn vm_hashmap_inventory_rejects_unknown_classification() {
    let root = make_quality_temp_dir("vm_hashmap_unknown_classification");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/tcp.rs",
        "use std::collections::HashMap;\n",
    );
    let rows = vec![VmHashMapInventoryRow {
        path: PathBuf::from("crates/terlan/src/runtime/vm/tcp.rs"),
        classification: "random-runtime-order".to_string(),
        owner: "vm".to_string(),
        notes: "bad".to_string(),
    }];
    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    let diagnostics = validate_vm_hashmap_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unsupported VM HashMap classification")),
        "expected classification diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies VM HashMap inventory rows cannot use placeholder ownership.
///
/// Inputs:
/// - One classified VM runtime `HashMap` reference with placeholder owner and
///   notes values.
///
/// Output:
/// - Diagnostic rejecting placeholder ownership.
///
/// Transformation:
/// - Prevents randomized VM map usage from being inventoried without a real
///   subsystem owner and actionable note.
#[test]
fn vm_hashmap_inventory_rejects_placeholder_owner_and_notes() {
    let root = make_quality_temp_dir("vm_hashmap_placeholder_owner_notes");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/tcp.rs",
        "use std::collections::HashMap;\n",
    );
    let rows = vec![VmHashMapInventoryRow {
        path: PathBuf::from("crates/terlan/src/runtime/vm/tcp.rs"),
        classification: "transport-registry".to_string(),
        owner: "todo".to_string(),
        notes: "fixme before release".to_string(),
    }];
    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    let diagnostics = validate_vm_hashmap_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must not use placeholder values")),
        "expected placeholder diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies allowed classification names cannot become placeholder buckets.
///
/// Inputs:
/// - Current allowed classification list plus an injected placeholder label.
///
/// Output:
/// - Current vocabulary is clean and the injected label is rejected.
///
/// Transformation:
/// - Prevents broad placeholder categories from becoming valid deterministic
///   HashMap inventory classifications.
#[test]
fn vm_hashmap_inventory_rejects_placeholder_allowed_classification_names() {
    let diagnostics = validate_allowed_classifications_have_no_placeholders();

    assert!(
        diagnostics.is_empty(),
        "allowed VM HashMap classifications must not contain placeholders: {diagnostics:?}"
    );

    let injected =
        validate_text_has_no_placeholder_value("allowed VM HashMap classification", "tbd-runtime");
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder inventory values")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}

/// Verifies VM HashMap inventory rows must stay path-sorted.
///
/// Inputs:
/// - Two valid VM runtime files containing `HashMap`.
/// - Inventory rows for both files in reverse path order.
///
/// Output:
/// - Diagnostic requiring path-sorted inventory rows.
///
/// Transformation:
/// - Keeps deterministic quality evidence stable under review and regeneration.
#[test]
fn vm_hashmap_inventory_rejects_unsorted_rows() {
    let root = make_quality_temp_dir("vm_hashmap_unsorted_rows");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/http_static.rs",
        "use std::collections::HashMap;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/acme_worker.rs",
        "use std::collections::HashMap;\n",
    );
    let rows = vec![
        VmHashMapInventoryRow {
            path: PathBuf::from("crates/terlan/src/runtime/vm/http_static.rs"),
            classification: "lookup-table".to_string(),
            owner: "vm-http".to_string(),
            notes: "lookup by static route".to_string(),
        },
        VmHashMapInventoryRow {
            path: PathBuf::from("crates/terlan/src/runtime/vm/acme_worker.rs"),
            classification: "handle-registry".to_string(),
            owner: "vm-acme".to_string(),
            notes: "lookup by renewal handle".to_string(),
        },
    ];
    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    let diagnostics = validate_vm_hashmap_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must be sorted by path")),
        "expected sorted inventory diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies unclassified VM runtime HashMap references fail the gate.
///
/// Inputs:
/// - One VM runtime source file that mentions `HashMap`.
/// - Empty inventory rows.
///
/// Output:
/// - Diagnostic naming the unclassified file.
///
/// Transformation:
/// - Ensures new randomized hash tables cannot enter VM-owned runtime code
///   without explicit ownership.
#[test]
fn vm_hashmap_inventory_rejects_unclassified_references() {
    let root = make_quality_temp_dir("vm_hashmap_unclassified_reference");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/http.rs",
        "use std::collections::HashMap;\n",
    );
    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    let diagnostics = validate_vm_hashmap_inventory(&root, &[], &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unclassified VM HashMap/RandomState reference")),
        "expected unclassified diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies direct RandomState use is treated as randomized VM hash-table drift.
///
/// Inputs:
/// - One VM runtime source file importing `RandomState` without an inventory row.
///
/// Output:
/// - Diagnostic naming the unclassified file.
///
/// Transformation:
/// - Extends the deterministic VM hash gate beyond `HashMap` spelling so
///   explicit randomized hash-state configuration cannot bypass the inventory.
#[test]
fn vm_hashmap_inventory_rejects_unclassified_random_state_references() {
    let root = make_quality_temp_dir("vm_hashmap_unclassified_random_state");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/http.rs",
        "use std::collections::hash_map::RandomState;\n",
    );
    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    let diagnostics = validate_vm_hashmap_inventory(&root, &[], &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unclassified VM HashMap/RandomState reference")),
        "expected unclassified RandomState diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies test files are ignored by the production VM HashMap gate.
///
/// Inputs:
/// - A VM adjacent test file containing `HashMap`.
///
/// Output:
/// - Empty reference set.
///
/// Transformation:
/// - Keeps the gate focused on runtime implementation semantics rather than
///   test fixture convenience.
#[test]
fn vm_hashmap_reference_scan_ignores_test_files() {
    let root = make_quality_temp_dir("vm_hashmap_ignores_tests");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/value_test.rs",
        "use std::collections::HashMap;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/http_test.rs",
        "include!(\"http_test_part_001.rs\");\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/http_test_part_001.rs",
        "use std::collections::HashMap;\n",
    );

    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    assert_eq!(references, BTreeSet::new());
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn vm_hashmap_reference_scan_attributes_parts_to_wrapper() {
    let root = make_quality_temp_dir("vm_hashmap_part_owner");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm.rs",
        "include!(\"vm_part_001.rs\");\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/vm_part_001.rs",
        "use std::collections::HashMap;\n",
    );

    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    assert_eq!(
        references,
        BTreeSet::from([PathBuf::from("crates/terlan/src/runtime/vm.rs")])
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies stale VM HashMap inventory rows fail the gate.
///
/// Inputs:
/// - One inventory row for a file that no longer mentions `HashMap`.
///
/// Output:
/// - Diagnostic naming the stale row.
///
/// Transformation:
/// - Keeps the inventory synchronized as runtime files move to deterministic
///   map types.
#[test]
fn vm_hashmap_inventory_rejects_stale_rows() {
    let root = make_quality_temp_dir("vm_hashmap_stale_row");
    write_file(
        &root,
        "crates/terlan/src/runtime/vm/tcp.rs",
        "use std::collections::BTreeMap;\n",
    );
    let rows = vec![VmHashMapInventoryRow {
        path: PathBuf::from("crates/terlan/src/runtime/vm/tcp.rs"),
        classification: "transport-registry".to_string(),
        owner: "vm-io".to_string(),
        notes: "old row".to_string(),
    }];
    let references = collect_vm_hashmap_reference_files(&root).expect("collect references");

    let diagnostics = validate_vm_hashmap_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("stale VM HashMap inventory row")),
        "expected stale diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Writes a repository fixture file.
fn write_file(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, text).expect("write fixture");
}

/// Creates a unique temporary repository root for quality gate tests.
fn make_quality_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan-vm-hashmap-quality-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("crates/terlan/src/runtime/vm")).expect("create fixture root");
    fs::create_dir_all(root.join("tools/quality")).expect("create quality root");
    root
}
