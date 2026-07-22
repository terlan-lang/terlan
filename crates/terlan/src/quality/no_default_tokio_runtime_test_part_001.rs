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
        "{INVENTORY_HEADER_ROW}crates/terlan/src/lsp/server.rs\teditor-tooling\tlsp\ttooling only\n"
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
        "crates/terlan/src/lsp/server.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/lsp/server.rs"),
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

/// Verifies inventory ownership cannot use placeholder values.
///
/// Inputs:
/// - One classified Tokio reference with placeholder owner and notes fields.
///
/// Output:
/// - Diagnostic rejecting placeholder inventory ownership.
///
/// Transformation:
/// - Prevents the Tokio removal inventory from becoming nominally classified
///   without a real subsystem owner or actionable note.
#[test]
fn tokio_inventory_rejects_placeholder_owner_and_notes() {
    let root = make_quality_temp_dir("tokio_placeholder_owner_notes");
    write_file(
        &root,
        "crates/terlan/src/lsp/server.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/lsp/server.rs"),
        classification: "editor-tooling".to_string(),
        owner: "todo".to_string(),
        notes: "fixme later".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must not use placeholder values")),
        "expected placeholder diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies the allowed classification vocabulary cannot become a placeholder.
///
/// Inputs:
/// - Current allowed classification list plus an injected placeholder label.
///
/// Output:
/// - Current vocabulary is clean and the injected label is rejected.
///
/// Transformation:
/// - Prevents broad placeholder categories from becoming valid Tokio inventory
///   escape hatches.
#[test]
fn tokio_inventory_rejects_placeholder_allowed_classification_names() {
    let diagnostics = validate_allowed_classifications_have_no_placeholders();

    assert!(
        diagnostics.is_empty(),
        "allowed Tokio classifications must not contain placeholders: {diagnostics:?}"
    );

    let injected =
        validate_text_has_no_placeholder_value("allowed Tokio classification", "todo-runtime");
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder inventory values")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
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

/// Verifies default VM runtime semantics cannot depend on Tokio.
///
/// Inputs:
/// - VM scheduler, timer, resource, wakeup, and CLI files that mention Tokio.
/// - Inventory rows trying to keep those references as migration debt.
///
/// Output:
/// - Stable diagnostics rejecting every VM-owned default runtime path.
///
/// Transformation:
/// - Locks the 0.0.7 rule that scheduling, timers, cancellation/wakeup,
///   resource lifecycle, and VM execution entrypoints must be VM-owned instead
///   of Tokio-backed.
#[test]
fn tokio_inventory_rejects_default_vm_runtime_semantic_paths() {
    let root = make_quality_temp_dir("tokio_default_vm_runtime_semantics");
    let paths = [
        "crates/terlan/src/runtime/vm/scheduler.rs",
        "crates/terlan/src/runtime/vm/timer.rs",
        "crates/terlan/src/runtime/vm/resource.rs",
        "crates/terlan/src/runtime/vm/wakeup.rs",
        "crates/terlan/src/vm/main.rs",
    ];
    for path in paths {
        write_file(&root, path, "tokio::time::sleep;\n");
    }
    let rows = paths
        .iter()
        .map(|path| TokioInventoryRow {
            path: PathBuf::from(path),
            classification: "migration-debt".to_string(),
            owner: "vm".to_string(),
            notes: "not allowed in default VM runtime semantics".to_string(),
        })
        .collect::<Vec<_>>();
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    for path in paths {
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.contains(path) && diagnostic.contains("VM-owned runtime paths")
            }),
            "expected VM runtime diagnostic for {path}: {diagnostics:?}"
        );
    }
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies Tokio classifications are restricted to their owning lanes.
///
/// Inputs:
/// - One serve runtime file that is incorrectly classified as editor tooling.
///
/// Output:
/// - Diagnostic rejecting the path/classification mismatch.
///
/// Transformation:
/// - Prevents future runtime Tokio references from being hidden behind broad
///   inventory categories that only belong to editor, generated, or quality
///   lanes.
#[test]
fn tokio_inventory_rejects_classification_scope_mismatch() {
    let root = make_quality_temp_dir("tokio_scope_mismatch");
    write_file(
        &root,
        "crates/terlan/src/commands/serve/mod.rs",
        "tokio::spawn(async {});\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/commands/serve/mod.rs"),
        classification: "editor-tooling".to_string(),
        owner: "http".to_string(),
        notes: "bad scope".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("classification `editor-tooling`")),
        "expected classification scope diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies quality-gate Tokio classifications stay inside quality sources.
///
/// Inputs:
/// - Std script, tools inventory, and non-allowlisted quality files that
///   mention Tokio.
/// - Inventory rows trying to classify them as quality-gate files.
///
/// Output:
/// - Diagnostics rejecting all non-allowlisted quality-gate paths.
///
/// Transformation:
/// - Prevents cleaned std scripts or repository tools from reintroducing Tokio
///   wording under the quality-gate self-reference exception.
#[test]
fn tokio_inventory_rejects_quality_gate_scope_outside_quality_sources() {
    let root = make_quality_temp_dir("tokio_quality_gate_scope");
    let paths = [
        "std/scripts/check_rust_backed_manifest.py",
        "tools/quality/tokio_runtime_inventory.tsv",
        "crates/terlan/src/quality/temporary_gate.rs",
    ];
    for path in paths {
        write_file(&root, path, "tokio-postgres\n");
    }
    let rows = paths
        .iter()
        .map(|path| TokioInventoryRow {
            path: PathBuf::from(path),
            classification: "quality-gate".to_string(),
            owner: "quality".to_string(),
            notes: "bad quality scope".to_string(),
        })
        .collect::<Vec<_>>();
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    for path in paths {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(path)),
            "expected quality-gate scope diagnostic for {path}: {diagnostics:?}"
        );
    }
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies retired reference-only Tokio classifications stay retired.
///
/// Inputs:
/// - A bind/probe source file that mentions Tokio.
/// - An inventory row using the old `reference-only` classification.
///
/// Output:
/// - Diagnostic rejecting the unknown classification.
///
/// Transformation:
/// - Prevents generated native binding probes from carrying stale Tokio wording
///   now that those docs describe VM-owned NativeBoundary workers.
#[test]
fn tokio_inventory_rejects_retired_reference_only_classification() {
    let root = make_quality_temp_dir("tokio_reference_only_retired");
    write_file(
        &root,
        "crates/terlan/src/commands/bind/polars_probe_files.rs",
        "tokio::runtime::Runtime;\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/commands/bind/polars_probe_files.rs"),
        classification: "reference-only".to_string(),
        owner: "bind".to_string(),
        notes: "retired generated probe classification".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("unsupported Tokio classification `reference-only`")),
        "expected retired reference-only diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies retired generated-summary Tokio classifications stay retired.
///
/// Inputs:
/// - A generated std summary file that mentions Tokio.
/// - An inventory row using the old `generated-summary` classification.
///
/// Output:
/// - Diagnostic rejecting the unsupported classification.
///
/// Transformation:
/// - Prevents generated std summaries from becoming a hiding place for
///   runtime-specific Tokio implementation details.
#[test]
fn tokio_inventory_rejects_retired_generated_summary_classification() {
    let root = make_quality_temp_dir("tokio_generated_summary_retired");
    write_file(
        &root,
        "std/summaries/std.db.Postgres.summary.json",
        "{\"owner\":\"tokio-postgres\"}\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("std/summaries/std.db.Postgres.summary.json"),
        classification: "generated-summary".to_string(),
        owner: "std-summary".to_string(),
        notes: "retired generated summary classification".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("unsupported Tokio classification `generated-summary`")
        }),
        "expected retired generated-summary diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies explicitly scoped test classifications remain accepted.
///
/// Inputs:
/// - One test file classified as test harness.
/// - One benchmark entrypoint classified as test harness.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Keeps the gate strict without blocking test-only Tokio lanes.
#[test]
fn tokio_inventory_accepts_scoped_test_lanes() {
    let root = make_quality_temp_dir("tokio_scoped_lanes");
    write_file(
        &root,
        "crates/terlan/src/commands/serve/serve_test.rs",
        "tokio::spawn(async {});\n",
    );
    write_file(
        &root,
        "crates/terlan/src/benchmark/main.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    let rows = vec![
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/commands/serve/serve_test.rs"),
            classification: "test-harness".to_string(),
            owner: "http-tests".to_string(),
            notes: "test only".to_string(),
        },
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/benchmark/main.rs"),
            classification: "test-harness".to_string(),
            owner: "benchmarks".to_string(),
            notes: "benchmark evidence only".to_string(),
        },
    ];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies serve TLS migration debt is limited to live ACME issuance.
///
/// Inputs:
/// - A serve TLS source fixture with the temporary runtime builder and ACME
///   issuance `block_on` call.
/// - A second fixture that also contains broader Tokio runtime usage.
///
/// Output:
/// - Valid live-ACME-only usage produces no diagnostics.
/// - Broader serve-side Tokio usage is rejected.
///
/// Transformation:
/// - Prevents the remaining serve TLS migration row from becoming a general
///   escape hatch for Tokio-backed HTTP serving.
#[test]
fn tokio_inventory_limits_serve_tls_migration_to_live_acme_issuance() {
    let root = make_quality_temp_dir("tokio_serve_tls_live_acme_only");
    write_file(
        &root,
        SERVE_TLS_MIGRATION_PATH,
        concat!(
            "let runtime = tokio::runtime::Builder::new_current_thread()\n",
            "    .enable_all()\n",
            "    .build()?;\n",
            "runtime.block_on(issue_acme_certificate_cache(plan));\n",
        ),
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from(SERVE_TLS_MIGRATION_PATH),
        classification: "migration-debt".to_string(),
        owner: "http".to_string(),
        notes: "temporary live ACME issuance".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics.is_empty(),
        "unexpected serve TLS migration diagnostics: {diagnostics:?}"
    );

    write_file(
        &root,
        SERVE_TLS_MIGRATION_PATH,
        concat!(
            "let runtime = tokio::runtime::Builder::new_current_thread()\n",
            "    .enable_all()\n",
            "    .build()?;\n",
            "tokio::spawn(async {});\n",
            "runtime.block_on(issue_acme_certificate_cache(plan));\n",
        ),
    );
    let references = collect_tokio_reference_files(&root).expect("collect invalid references");

    let invalid_diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        invalid_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("exactly one `tokio::` use")),
        "expected serve TLS Tokio scope diagnostic: {invalid_diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies benchmark test-harness scope stays on the benchmark entrypoint.
///
/// Inputs:
/// - A benchmark helper module that mentions Tokio.
/// - An inventory row trying to classify it as test harness.
///
/// Output:
/// - Diagnostic rejecting the helper module.
///
/// Transformation:
/// - Keeps the benchmark Tokio lane constrained to the recorded benchmark
///   entrypoint instead of the whole benchmark source tree.
#[test]
fn tokio_inventory_rejects_benchmark_helper_as_test_harness() {
    let root = make_quality_temp_dir("tokio_benchmark_helper_scope");
    write_file(
        &root,
        "crates/terlan/src/benchmark/http_runtime_lane.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    let rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/benchmark/http_runtime_lane.rs"),
        classification: "test-harness".to_string(),
        owner: "benchmarks".to_string(),
        notes: "bad broad benchmark lane".to_string(),
    }];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let diagnostics = validate_tokio_inventory(&root, &rows, &references);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("crates/terlan/src/benchmark/http_runtime_lane.rs")),
        "expected benchmark helper test-harness diagnostic: {diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies LSP Tokio usage stays classified as editor tooling or tests.
///
/// Inputs:
/// - One LSP runtime source file and one LSP test file that mention Tokio.
/// - Matching inventory rows with editor-tooling and test-harness
///   classifications.
/// - One invalid LSP source row classified as migration debt.
/// - One non-entrypoint LSP source row classified as editor tooling.
///
/// Output:
/// - Valid editor/test classifications produce no diagnostics.
/// - Invalid default-runtime-style classification is rejected.
/// - Extra LSP source files cannot use the editor-tooling exception.
///
/// Transformation:
/// - Locks the 0.0.7 rule that LSP may retain Tokio only as editor tooling,
///   never as a default runtime dependency lane.
#[test]
fn tokio_inventory_keeps_lsp_tokio_as_editor_tooling_or_tests() {
    let root = make_quality_temp_dir("tokio_lsp_editor_tooling");
    write_file(
        &root,
        "crates/terlan/src/lsp/server.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    write_file(
        &root,
        "crates/terlan/src/lsp/lib_test.rs",
        "tokio::spawn(async {});\n",
    );
    let valid_rows = vec![
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/lsp/server.rs"),
            classification: "editor-tooling".to_string(),
            owner: "lsp".to_string(),
            notes: "editor transport only".to_string(),
        },
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/lsp/lib_test.rs"),
            classification: "test-harness".to_string(),
            owner: "lsp-tests".to_string(),
            notes: "editor tests only".to_string(),
        },
    ];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let valid_diagnostics = validate_tokio_inventory(&root, &valid_rows, &references);

    assert!(
        valid_diagnostics.is_empty(),
        "unexpected valid LSP diagnostics: {valid_diagnostics:?}"
    );

    let invalid_rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/lsp/server.rs"),
        classification: "migration-debt".to_string(),
        owner: "lsp".to_string(),
        notes: "bad runtime lane".to_string(),
    }];
    let invalid_diagnostics = validate_tokio_inventory(&root, &invalid_rows, &references);

    assert!(
        invalid_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("classification `migration-debt`")),
        "expected LSP migration-debt diagnostic: {invalid_diagnostics:?}"
    );

    write_file(
        &root,
        "crates/terlan/src/lsp/transport.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    let references = collect_tokio_reference_files(&root).expect("collect invalid references");
    let invalid_extra_rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/lsp/transport.rs"),
        classification: "editor-tooling".to_string(),
        owner: "lsp".to_string(),
        notes: "bad broad editor tooling lane".to_string(),
    }];
    let invalid_extra_diagnostics =
        validate_tokio_inventory(&root, &invalid_extra_rows, &references);

    assert!(
        invalid_extra_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("crates/terlan/src/lsp/transport.rs")),
        "expected LSP extra-module editor-tooling diagnostic: {invalid_extra_diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies Postgres Tokio usage has no inventory exemption.
///
/// Inputs:
/// - Native Postgres boundary and row files that mention Tokio.
/// - Empty inventory, followed by an attempted retired boundary exemption.
///
/// Output:
/// - Both files are reported as unclassified.
/// - The retired maintained-client classification is rejected.
///
/// Transformation:
/// - Locks Postgres execution to VM-owned worker scheduling with no hidden
///   adapter runtime exception.
#[test]
fn tokio_inventory_rejects_postgres_runtime_references() {
    let root = make_quality_temp_dir("tokio_postgres_forbidden");
    write_file(
        &root,
        "crates/terlan/src/runtime/native/postgres.rs",
        "tokio::runtime::Builder::new_current_thread();\n",
    );
    write_file(
        &root,
        "crates/terlan/src/runtime/native/postgres/row.rs",
        "tokio_postgres::Row;\n",
    );
    let references = collect_tokio_reference_files(&root).expect("collect references");
    let diagnostics = validate_tokio_inventory(&root, &[], &references);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.contains("runtime/native/postgres.rs: unclassified Tokio reference")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.contains("runtime/native/postgres/row.rs: unclassified Tokio reference")
    }));

    let retired_rows = vec![TokioInventoryRow {
        path: PathBuf::from("crates/terlan/src/runtime/native/postgres.rs"),
        classification: "maintained-client-boundary".to_string(),
        owner: "postgres".to_string(),
        notes: "retired adapter exemption".to_string(),
    }];
    let retired_diagnostics = validate_tokio_inventory(&root, &retired_rows, &references);
    assert!(
        retired_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("unsupported Tokio classification")),
        "expected retired classification diagnostic: {retired_diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies benchmark HTTP traffic stays off the Tokio client lane.
///
/// Inputs:
/// - The real benchmark binary source.
///
/// Output:
/// - Test passes when benchmark source contains no Tokio wording or client
///   runtime imports.
///
/// Transformation:
/// - Keeps the benchmark lane useful for VM-vs-legacy comparisons without
///   making benchmarks another default Tokio runtime surface.
#[test]
fn tokio_inventory_rejects_benchmark_http_client_runtime_code() {
    let root = repo_root();
    let benchmark = fs::read_to_string(root.join("crates/terlan/src/benchmark/main.rs"))
        .expect("read benchmark main");

    for forbidden in [
        "hyper_util::client",
        "TokioExecutor",
        "tokio::runtime::Builder",
        "tokio::time::sleep",
        "BodyExt",
        "Full<Bytes>",
        "tokio",
    ] {
        assert!(
            !benchmark.contains(forbidden),
            "benchmark HTTP lane must not reintroduce `{forbidden}`"
        );
    }
}
