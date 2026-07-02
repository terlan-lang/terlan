use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

const INVENTORY_HEADER_ROW: &str = "path\tclassification\towner\tnotes\n";

/// Verifies the Tokio inventory parser accepts valid rows.
///
/// Inputs:
/// - A minimal TSV inventory with one classified source file.
///
/// Output:
/// - One parsed inventory row.
///
/// Transformation:
/// - Locks the checked inventory shape used by the repository gate.
#[test]
fn tokio_inventory_parser_accepts_valid_rows() {
    let rows = parse_tokio_inventory(&format!(
        "{INVENTORY_HEADER_ROW}crates/terlan/src/lsp/mod.rs\teditor-tooling\tlsp\ttooling only\n"
    ))
    .expect("parse inventory");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].classification, "editor-tooling");
}

/// Verifies unsupported inventory classifications are rejected.
///
/// Inputs:
/// - One inventory row with a made-up classification.
///
/// Output:
/// - Diagnostic naming the unsupported classification.
///
/// Transformation:
/// - Prevents unchecked categories from weakening the Tokio removal contract.
#[test]
fn tokio_inventory_rejects_unknown_classification() {
    let root = make_quality_temp_dir("tokio_unknown_classification");
    write_file(
        &root,
        "crates/terlan/src/lsp/mod.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/lsp/mod.rs"),
        classification: "default-runtime".to_string(),
        owner: "lsp".to_string(),
        notes: "bad".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unsupported Tokio classification")),
        "expected classification diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies unclassified Tokio references fail the gate.
///
/// Inputs:
/// - One source file that mentions Tokio.
/// - Empty inventory rows.
///
/// Output:
/// - Diagnostic naming the unclassified file.
///
/// Transformation:
/// - Ensures new Tokio usage cannot enter the tree without explicit ownership.
#[test]
fn tokio_inventory_rejects_unclassified_references() {
    let root = make_quality_temp_dir("tokio_unclassified_reference");
    write_file(
        &root,
        "crates/terlan/src/commands/serve/mod.rs",
        "tokio::spawn(async {});\n",
    );
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &[], &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unclassified Tokio reference")),
        "expected unclassified diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies VM-owned runtime paths cannot be classified as retained Tokio.
///
/// Inputs:
/// - One VM runtime file that mentions Tokio.
/// - One inventory row trying to classify it.
///
/// Output:
/// - Diagnostic rejecting Tokio in VM-owned runtime paths.
///
/// Transformation:
/// - Protects the core VM implementation from accidental Tokio dependency
///   creep while other migration lanes remain inventoried.
#[test]
fn tokio_inventory_rejects_vm_runtime_paths() {
    let root = make_quality_temp_dir("tokio_vm_runtime_path");
    write_file(
        &root,
        "crates/terlan/src/vm/runtime.rs",
        "tokio::time::sleep;\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/vm/runtime.rs"),
        classification: "migration-debt".to_string(),
        owner: "vm".to_string(),
        notes: "not allowed".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("VM-owned runtime paths")),
        "expected VM runtime diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies the full gate accepts matching inventory and scanned files.
///
/// Inputs:
/// - Temporary inventory and one classified LSP source file.
///
/// Output:
/// - Success summary with one row and one scanned reference.
///
/// Transformation:
/// - Exercises the disk-backed gate used by `make no-default-tokio-runtime-check`.
#[test]
fn no_default_tokio_runtime_gate_accepts_matching_inventory() {
    let root = make_quality_temp_dir("tokio_matching_inventory");
    write_file(
        &root,
        "crates/terlan/src/lsp/mod.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    write_file(
        &root,
        TOKIO_INVENTORY_PATH,
        &format!(
            "{INVENTORY_HEADER_ROW}crates/terlan/src/lsp/mod.rs\teditor-tooling\tlsp\ttooling only\n"
        ),
    );

    let summary = run_no_default_tokio_runtime(&root).expect("run gate");

    assert_eq!(summary.inventory_row_count, 1);
    assert_eq!(summary.scanned_reference_count, 1);
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Writes a fixture file, creating parent directories first.
fn write_file(root: &std::path::Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture dir");
    }
    fs::write(path, text).expect("write fixture file");
}

/// Creates a unique temporary directory for quality tests.
fn make_quality_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "terlan_quality_{label}_{}_{}",
        std::process::id(),
        nanos
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create quality temp dir");
    path
}
