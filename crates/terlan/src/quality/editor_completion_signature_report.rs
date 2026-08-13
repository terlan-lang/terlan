use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::editor_report_selector::EditorReportSelector as SelectorSpec;
use super::support::{make_target_body, write_json_report};
use crate::terlan_quality::placeholder_terms::{
    placeholder_entry_diagnostics, selector_evidence_placeholder_diagnostics,
};
use crate::terlan_quality::roadmap_gate_integrity::parse_make_list_variable_values;
use crate::terlan_quality::{render_failure, QualityResult};

const REPORT_PATH: &str = "target/quality/editor-completion-signature-report.json";
const TARGET: &str = "editor-completion-signature-check";
const LSP_TEST_PATH: &str = "crates/terlan/src/lsp/lib_test.rs";
const EXPECTED_SELECTOR_COUNT: usize = 12;
const EXPECTED_REPORT_CATEGORY_COUNT: usize = 11;
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd", "pending"];

const REQUIRED_SELECTORS: &[SelectorSpec] = &[
    SelectorSpec {
        fixture: "completion_items_include_local_and_imported_shapes_and_functions",
        category: "completion fixtures",
        evidence: &[
            "local/imported shape completions",
            "local/imported function completions",
            "constructor/type/trait completions",
            "receiver field and method completions",
            "private provider symbol rejection",
        ],
    },
    SelectorSpec {
        fixture: "completion_request_uses_latest_changed_document_version",
        category: "stale-cache rejections",
        evidence: &[
            "didChange completion refresh",
            "stale local binding rejection",
            "latest buffer snapshot completion",
        ],
    },
    SelectorSpec {
        fixture: "completion_request_handles_incomplete_syntax_without_stale_items",
        category: "incomplete syntax fixtures",
        evidence: &[
            "parse-diagnostic completion request",
            "empty completion response for incomplete syntax",
            "no stale items for broken buffers",
        ],
    },
    SelectorSpec {
        fixture: "completion_ranks_local_symbol_before_imported_ambiguous_symbol",
        category: "ranking decisions",
        evidence: &[
            "ambiguous local/imported function label",
            "local declaration ranked before imported declaration",
            "provider-qualified imported detail retained",
        ],
    },
    SelectorSpec {
        fixture: "completion_preserves_overloaded_receiver_method_items",
        category: "overloaded method fixtures",
        evidence: &[
            "same-name receiver method overloads",
            "distinct receiver method arity details",
            "no completion deduplication for overloads",
        ],
    },
    SelectorSpec {
        fixture: "completion_uses_formatter_shifted_positions_for_local_functions",
        category: "formatter span shift fixtures",
        evidence: &[
            "formatter-split function call chain",
            "formatted cursor position completion",
            "local function after formatter layout shift",
        ],
    },
    SelectorSpec {
        fixture: "completion_uses_generated_typi_interface_summary_provenance",
        category: "generated binding fixtures",
        evidence: &[
            "generated typi interface summary discovery",
            "package-qualified function completion detail",
            "generated summary completion provenance",
        ],
    },
    SelectorSpec {
        fixture: "completion_rejects_deleted_generated_typi_summary",
        category: "stale-cache rejections",
        evidence: &[
            "deleted generated typi summary rejection",
            "stale generated package completion suppression",
            "local completion preservation after package metadata deletion",
        ],
    },
    SelectorSpec {
        fixture: "completion_rejects_mixed_target_profile_imported_suggestions",
        category: "target-profile checks",
        evidence: &[
            "mixed target-profile imported completion rejection",
            "target-specific std completion suppression",
            "local completion preservation during target conflict",
        ],
    },
    SelectorSpec {
        fixture: "signature_help_request_returns_local_function_signature",
        category: "signature-help fixtures",
        evidence: &[
            "local function signature help",
            "generic function signature label",
            "imported generic function signature help",
            "local receiver-method signature help",
            "imported receiver-method signature help",
            "active argument selection",
        ],
    },
    SelectorSpec {
        fixture: "signature_parameter_label_preserves_mutability_patterns_and_defaults",
        category: "signature-help fixtures",
        evidence: &[
            "mutable parameter labels",
            "pattern parameter labels",
            "default value labels",
        ],
    },
    SelectorSpec {
        fixture: "inlay_hint_request_returns_literal_binding_type_hint",
        category: "inlay-hint fixtures",
        evidence: &[
            "literal let-binding type hints",
            "local function parameter-name hints",
            "imported function parameter-name hints",
            "imported function inlay provenance tooltips",
            "receiver-method parameter-name hints",
            "defaulted argument inlay hints",
        ],
    },
];

const REPORT_NOTES: &[&str] = &[
    "Ranking remains validated by the completion fixture ordering assertions.",
    "Stale-cache rejection is validated through didChange and deleted generated-summary fixtures.",
    "Target-profile checks are validated through mixed-target imported-completion suppression.",
    "Editor parity is represented by LSP protocol-level fixtures.",
];

/// Summary produced by the editor completion/signature report gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCompletionSignatureReportSummary {
    pub selector_count: usize,
    pub category_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct EditorCompletionSignatureReport {
    gate: &'static str,
    report_schema: &'static str,
    completion_fixtures: Vec<&'static str>,
    ranking_decisions: Vec<&'static str>,
    signature_help_fixtures: Vec<&'static str>,
    inlay_hint_fixtures: Vec<&'static str>,
    stale_cache_rejections: Vec<&'static str>,
    incomplete_syntax_fixtures: Vec<&'static str>,
    overloaded_method_fixtures: Vec<&'static str>,
    formatter_span_shift_fixtures: Vec<&'static str>,
    generated_binding_fixtures: Vec<&'static str>,
    target_profile_checks: Vec<&'static str>,
    editor_parity_notes: Vec<&'static str>,
}

/// Runs the editor completion/signature report gate.
///
/// Inputs:
/// - `root`: golden repository root containing `Makefile` and editor gates.
///
/// Output:
/// - A summary with selector/category counts and the written report path.
/// - An error when the Make target or required exact selectors drift.
///
/// Transformation:
/// - Reads the Make include graph, validates the editor gate body, and writes a
///   JSON report mirroring the current executable evidence.
pub fn run_editor_completion_signature_report(
    root: &Path,
) -> QualityResult<EditorCompletionSignatureReportSummary> {
    let makefile = read_make_graph(root)?;
    let fixtures = read_text(root, LSP_TEST_PATH)?;
    let mut diagnostics = validate_gate_and_fixtures(&makefile, &fixtures);
    diagnostics.extend(validate_selector_inventory(REQUIRED_SELECTORS));
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "editor-completion-signature-report",
            &diagnostics,
        ));
    }

    let report = build_report();
    diagnostics.extend(validate_report_inventory(&report));
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "editor-completion-signature-report",
            &diagnostics,
        ));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path, &report)?;

    Ok(EditorCompletionSignatureReportSummary {
        selector_count: REQUIRED_SELECTORS.len(),
        category_count: report_category_count(&report),
        report_path,
    })
}

fn build_report() -> EditorCompletionSignatureReport {
    EditorCompletionSignatureReport {
        gate: TARGET,
        report_schema: "editor-completion-signature-report-v1",
        completion_fixtures: fixtures_for("completion fixtures"),
        ranking_decisions: fixtures_for("ranking decisions"),
        signature_help_fixtures: fixtures_for("signature-help fixtures"),
        inlay_hint_fixtures: fixtures_for("inlay-hint fixtures"),
        stale_cache_rejections: stale_cache_rejections(),
        incomplete_syntax_fixtures: fixtures_for("incomplete syntax fixtures"),
        overloaded_method_fixtures: fixtures_for("overloaded method fixtures"),
        formatter_span_shift_fixtures: fixtures_for("formatter span shift fixtures"),
        generated_binding_fixtures: fixtures_for("generated binding fixtures"),
        target_profile_checks: fixtures_for("target-profile checks"),
        editor_parity_notes: REPORT_NOTES.to_vec(),
    }
}

fn validate_selector_inventory(selectors: &[SelectorSpec]) -> Vec<String> {
    if selectors.len() == EXPECTED_SELECTOR_COUNT {
        Vec::new()
    } else {
        vec![format!(
            "`{TARGET}` must keep {EXPECTED_SELECTOR_COUNT} exact completion/signature selectors; found {}",
            selectors.len()
        )]
    }
}

fn validate_report_inventory(report: &EditorCompletionSignatureReport) -> Vec<String> {
    let category_count = report_category_count(report);
    if category_count == EXPECTED_REPORT_CATEGORY_COUNT {
        Vec::new()
    } else {
        vec![format!(
            "`{TARGET}` report must keep {EXPECTED_REPORT_CATEGORY_COUNT} populated evidence categories; found {category_count}"
        )]
    }
}

fn fixtures_for(category: &str) -> Vec<&'static str> {
    REQUIRED_SELECTORS
        .iter()
        .filter(|selector| selector.category == category)
        .flat_map(|selector| selector.evidence.iter().copied())
        .collect()
}

fn stale_cache_rejections() -> Vec<&'static str> {
    fixtures_for("stale-cache rejections")
}

fn report_category_count(report: &EditorCompletionSignatureReport) -> usize {
    [
        !report.completion_fixtures.is_empty(),
        !report.ranking_decisions.is_empty(),
        !report.signature_help_fixtures.is_empty(),
        !report.inlay_hint_fixtures.is_empty(),
        !report.stale_cache_rejections.is_empty(),
        !report.incomplete_syntax_fixtures.is_empty(),
        !report.overloaded_method_fixtures.is_empty(),
        !report.formatter_span_shift_fixtures.is_empty(),
        !report.generated_binding_fixtures.is_empty(),
        !report.target_profile_checks.is_empty(),
        !report.editor_parity_notes.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
}

/// Validates canonical suite ownership, report wiring, and LSP fixture presence.
fn validate_gate_and_fixtures(makefile: &str, fixtures: &str) -> Vec<String> {
    let Some(body) = make_target_body(makefile, TARGET) else {
        return vec![format!("Makefile: missing target `{TARGET}`")];
    };

    let mut diagnostics = Vec::new();
    if !body.contains("editor-completion-signature-report") {
        diagnostics.push(format!(
            "Makefile: `{TARGET}` must run its completion/signature report"
        ));
    }
    if !parse_make_list_variable_values(makefile, "COMPLETED_SLICE_RUST_GATES")
        .iter()
        .any(|gate| gate == TARGET)
    {
        diagnostics.push(format!(
            "Makefile: `COMPLETED_SLICE_RUST_GATES` must own `{TARGET}`"
        ));
    }
    for selector in REQUIRED_SELECTORS {
        if !fixtures.contains(&format!("fn {}", selector.fixture)) {
            diagnostics.push(format!(
                "{LSP_TEST_PATH}: missing completion/signature fixture `{}`",
                selector.fixture
            ));
        }
    }
    diagnostics
}

fn validate_no_placeholder_report_entries() -> Vec<String> {
    selector_evidence_placeholder_diagnostics(
        REQUIRED_SELECTORS
            .iter()
            .map(|selector| (selector.fixture, selector.evidence)),
        REPORT_NOTES,
        validate_entries_for_placeholder_terms,
    )
}

fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    placeholder_entry_diagnostics(
        label,
        entries,
        PLACEHOLDER_REPORT_TERMS,
        |label, entry, term| {
            format!(
            "editor completion/signature {label} entry `{entry}` uses placeholder term `{term}`"
        )
        },
    )
}

fn read_make_graph(root: &Path) -> QualityResult<String> {
    let makefile = read_text(root, "Makefile")?;
    let mut graph = makefile.clone();
    for include in makefile.lines().filter_map(include_path) {
        graph.push('\n');
        graph.push_str(&read_text(root, include)?);
    }
    Ok(graph)
}

fn include_path(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("include ")
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

fn write_report(path: &Path, report: &EditorCompletionSignatureReport) -> QualityResult<()> {
    write_json_report(path, report)
}

#[cfg(test)]
#[path = "editor_completion_signature_report_test.rs"]
#[cfg(test)]
mod editor_completion_signature_report_test;
