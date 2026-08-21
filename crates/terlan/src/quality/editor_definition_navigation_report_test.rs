use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    build_report, run_editor_definition_navigation_report, validate_entries_for_placeholder_terms,
    validate_gate_and_fixtures, validate_no_placeholder_report_entries, validate_report_inventory,
    validate_selector_inventory, EXPECTED_REPORT_CATEGORY_COUNT, EXPECTED_SELECTOR_COUNT,
    REQUIRED_SELECTORS, TARGET,
};

#[test]
fn editor_definition_navigation_report_writes_expected_artifact() {
    let root = temp_root("editor-definition-navigation-report-ok");
    fs::create_dir_all(root.join("editors")).expect("create editors dir");
    fs::create_dir_all(root.join("crates/terlan/src/lsp")).expect("create lsp dir");
    fs::write(root.join("Makefile"), "include editors/editor.mk\n").expect("write Makefile");
    fs::write(root.join("editors/editor.mk"), editor_gate_body()).expect("write editor makefile");
    fs::write(
        root.join("crates/terlan/src/lsp/lib_test.rs"),
        fixture_source(),
    )
    .expect("write fixtures");

    let summary = run_editor_definition_navigation_report(&root).expect("report check passes");
    let report = fs::read_to_string(summary.report_path).expect("read report");

    assert_eq!(summary.selector_count, EXPECTED_SELECTOR_COUNT);
    assert_eq!(summary.category_count, EXPECTED_REPORT_CATEGORY_COUNT);
    assert!(report.contains("\"report_schema\": \"editor-definition-navigation-report-v1\""));
    assert!(report.contains("same-document definition request"));
    assert!(report.contains("provider-summary function target"));
    assert!(report.contains("type-definition provider capability"));
    assert!(report.contains("trait and impl reference filtering"));
    assert!(report.contains("no arbitrary provider target"));
    assert!(report.contains("packaged-summary type target"));
    assert!(report.contains("template definition request rejection"));
    assert!(report.contains("post-format source offset target"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
    fs::remove_dir_all(root).expect("remove temp root");
}

#[test]
fn editor_definition_navigation_report_rejects_selector_count_drift() {
    let diagnostics =
        validate_selector_inventory(&REQUIRED_SELECTORS[..EXPECTED_SELECTOR_COUNT - 1]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must keep 39 exact navigation selectors")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_definition_navigation_report_accepts_current_report_category_inventory() {
    let report = build_report();
    let diagnostics = validate_report_inventory(&report);

    assert_eq!(diagnostics, Vec::<String>::new());
}

#[test]
fn editor_definition_navigation_report_rejects_category_count_drift() {
    let mut report = build_report();
    report.editor_parity_notes.clear();

    let diagnostics = validate_report_inventory(&report);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must keep 9 populated evidence categories")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_definition_navigation_report_rejects_missing_selector() {
    let body = editor_gate_body();
    let fixtures = format!("fn {}() {{}}\n", REQUIRED_SELECTORS[0].fixture);

    let diagnostics = validate_gate_and_fixtures(&body, &fixtures);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .contains("definition_locations_resolve_imported_trait_reference")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_definition_navigation_report_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "definition navigation report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "editor parity notes",
        &["editor placeholder navigation note"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}

fn editor_gate_body() -> String {
    format!("{TARGET}:\n\tterlan-quality editor-definition-navigation-report\n")
}

fn fixture_source() -> String {
    let mut body = String::new();
    for selector in REQUIRED_SELECTORS {
        body.push_str("fn ");
        body.push_str(selector.fixture);
        body.push_str("() {}\n");
    }
    body
}

fn temp_root(prefix: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}
