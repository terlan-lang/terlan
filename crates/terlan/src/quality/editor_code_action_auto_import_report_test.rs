use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    build_report, run_editor_code_action_auto_import_report,
    validate_entries_for_placeholder_terms, validate_fixture_inventory, validate_gate_and_fixtures,
    validate_no_placeholder_report_entries, validate_report_inventory, EXPECTED_FIXTURE_COUNT,
    EXPECTED_REPORT_CATEGORY_COUNT, REQUIRED_FIXTURES, TARGET,
};

#[test]
fn editor_code_action_auto_import_report_writes_expected_artifact() {
    let root = temp_root("editor-code-action-auto-import-report-ok");
    fs::create_dir_all(root.join("editors")).expect("create editors dir");
    fs::create_dir_all(root.join("crates/terlan/src/lsp")).expect("create lsp dir");
    fs::write(root.join("Makefile"), "include editors/editor.mk\n").expect("write Makefile");
    fs::write(root.join("editors/editor.mk"), editor_gate_body()).expect("write editor makefile");
    fs::write(
        root.join("crates/terlan/src/lsp/import_actions_test.rs"),
        import_actions_fixture_source(),
    )
    .expect("write import action fixtures");

    let summary = run_editor_code_action_auto_import_report(&root).expect("report check passes");
    let report = fs::read_to_string(summary.report_path).expect("read report");

    assert_eq!(summary.fixture_count, EXPECTED_FIXTURE_COUNT);
    assert_eq!(summary.category_count, EXPECTED_REPORT_CATEGORY_COUNT);
    assert!(report.contains("\"report_schema\": \"editor-code-action-auto-import-report-v1\""));
    assert!(report.contains("LSP workspace edit for Vector import"));
    assert!(report.contains("private provider function rejection"));
    assert!(report.contains("ambiguous public functions produce one choice per provider"));
    assert!(report.contains("module docs preserved while inserting imports"));
    assert!(report.contains("generated typi callable binding import"));
    assert!(report.contains("missing original re-export provider rejection"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
    fs::remove_dir_all(root).expect("remove temp root");
}

#[test]
fn editor_code_action_auto_import_report_rejects_fixture_count_drift() {
    let diagnostics = validate_fixture_inventory(&REQUIRED_FIXTURES[..EXPECTED_FIXTURE_COUNT - 1]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must keep 21 exact auto-import fixtures")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_code_action_auto_import_report_rejects_category_count_drift() {
    let mut report = build_report();
    report.ambiguity_rankings.clear();

    let diagnostics = validate_report_inventory(&report);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must keep 7 populated evidence categories")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_code_action_auto_import_report_rejects_missing_fixture() {
    let body = editor_gate_body();
    let fixtures = "fn diagnostic_import_actions_recognize_unknown_constructor() {}\n";

    let diagnostics = validate_gate_and_fixtures(&body, fixtures);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("import_candidate_keeps_ambiguous_function_choices")),
        "{diagnostics:?}"
    );
}

#[test]
fn editor_code_action_auto_import_report_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "auto-import report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected =
        validate_entries_for_placeholder_terms("applied edits", &["placeholder auto-import edit"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}

fn editor_gate_body() -> String {
    format!("{TARGET}:\n\tterlan-quality editor-code-action-auto-import-report\n")
}

fn import_actions_fixture_source() -> String {
    let mut source = String::new();
    for fixture in REQUIRED_FIXTURES {
        source.push_str("#[test]\nfn ");
        source.push_str(fixture.name);
        source.push_str("() {}\n");
    }
    source
}

fn temp_root(prefix: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}
