use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    build_report, run_editor_completion_signature_report, validate_entries_for_placeholder_terms,
    validate_gate_and_fixtures, validate_no_placeholder_report_entries, validate_report_inventory,
    validate_selector_inventory, EXPECTED_REPORT_CATEGORY_COUNT, EXPECTED_SELECTOR_COUNT,
    REQUIRED_SELECTORS,
};

#[test]
fn editor_completion_signature_report_writes_expected_artifact() {
    let root = temp_root("editor-completion-signature-report-ok");
    fs::create_dir_all(root.join("editors")).expect("create editors dir");
    fs::create_dir_all(root.join("crates/terlan/src/lsp")).expect("create lsp dir");
    fs::write(
        root.join("Makefile"),
        "COMPLETED_SLICE_RUST_GATES := editor-completion-signature-check\ninclude editors/editor.mk\n",
    )
    .expect("write Makefile");
    fs::write(
        root.join("editors/editor.mk"),
        "editor-completion-signature-check:\n\tterlan-quality editor-completion-signature-report\n",
    )
    .expect("write editor makefile");
    fs::write(
        root.join("crates/terlan/src/lsp/lib_test.rs"),
        fixture_source(),
    )
    .expect("write fixtures");

    let summary = run_editor_completion_signature_report(&root).expect("report check passes");
    let report = fs::read_to_string(summary.report_path).expect("read report");

    assert_eq!(summary.selector_count, EXPECTED_SELECTOR_COUNT);
    assert_eq!(summary.category_count, EXPECTED_REPORT_CATEGORY_COUNT);
    assert!(report.contains("\"report_schema\": \"editor-completion-signature-report-v1\""));
    assert!(report.contains("local declaration ranked before imported declaration"));
    assert!(report.contains("no completion deduplication for overloads"));
    assert!(report.contains("local function after formatter layout shift"));
    assert!(report.contains("package-qualified function completion detail"));
    assert!(report.contains("deleted generated typi summary rejection"));
    assert!(report.contains("stale generated package completion suppression"));
    assert!(report.contains("mixed target-profile imported completion rejection"));
    assert!(report.contains("generic function signature label"));
    assert!(report.contains("imported generic function signature help"));
    assert!(report.contains("defaulted argument inlay hints"));
    assert!(report.contains("imported function inlay provenance tooltips"));
    assert!(report.contains("mutable parameter labels"));
    assert!(report.contains("stale local binding rejection"));
    assert!(report.contains("empty completion response for incomplete syntax"));
    assert!(!report.contains("pending:"));
    fs::remove_dir_all(root).expect("remove temp root");
}

#[test]
fn editor_completion_signature_report_rejects_selector_count_drift() {
    let diagnostics =
        validate_selector_inventory(&REQUIRED_SELECTORS[..EXPECTED_SELECTOR_COUNT - 1]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("must keep 12 exact completion/signature selectors")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_completion_signature_report_rejects_category_count_drift() {
    let mut report = build_report();
    report.editor_parity_notes.clear();

    let diagnostics = validate_report_inventory(&report);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must keep 11 populated evidence categories")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_completion_signature_report_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert_eq!(diagnostics, Vec::<String>::new());

    let injected = validate_entries_for_placeholder_terms(
        "completion parity notes",
        &["pending completion support"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}

#[test]
fn editor_completion_signature_report_rejects_missing_selector() {
    let body = "COMPLETED_SLICE_RUST_GATES := editor-completion-signature-check\n\
editor-completion-signature-check:\n\tterlan-quality editor-completion-signature-report\n";
    let fixtures = format!("fn {}() {{}}\n", REQUIRED_SELECTORS[0].fixture);

    let diagnostics = validate_gate_and_fixtures(body, &fixtures);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("signature_help_request_returns")),
        "{diagnostics:?}"
    );
}

fn fixture_source() -> String {
    REQUIRED_SELECTORS
        .iter()
        .map(|selector| format!("fn {}() {{}}\n", selector.fixture))
        .collect()
}

fn temp_root(prefix: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}
