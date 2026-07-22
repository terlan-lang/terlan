use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::terlan_quality::roadmap_gate_integrity::parse_make_list_variable_values;
use crate::terlan_quality::{render_failure, QualityResult};

const REPORT_PATH: &str = "target/quality/editor-code-action-auto-import-report.json";
const TARGET: &str = "editor-code-action-auto-import-check";
const IMPORT_ACTION_TEST_PATH: &str = "crates/terlan/src/lsp/import_actions_test.rs";
const EXPECTED_FIXTURE_COUNT: usize = 21;
const EXPECTED_REPORT_CATEGORY_COUNT: usize = 7;
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_FIXTURES: &[FixtureSpec] = &[
    FixtureSpec {
        name: "diagnostic_import_actions_recognize_unknown_constructor",
        category: "diagnostic parsing",
        evidence: "unknown constructor diagnostic with spaced arity",
    },
    FixtureSpec {
        name: "diagnostic_import_actions_recognize_compact_unknown_constructor",
        category: "diagnostic parsing",
        evidence: "unknown constructor diagnostic with compact arity",
    },
    FixtureSpec {
        name: "diagnostic_import_action_contains_workspace_edit",
        category: "applied edits",
        evidence: "LSP workspace edit for Vector import",
    },
    FixtureSpec {
        name: "import_candidate_inserts_missing_vector_import",
        category: "applied edits",
        evidence: "canonical missing Vector module import",
    },
    FixtureSpec {
        name: "import_candidate_preserves_leading_module_docs",
        category: "formatter parity",
        evidence: "module docs preserved while inserting imports",
    },
    FixtureSpec {
        name: "import_candidate_replaces_wrong_vector_import",
        category: "rejected edits",
        evidence: "stale same-leaf Vector import replacement",
    },
    FixtureSpec {
        name: "import_candidate_skips_already_imported_vector",
        category: "rejected edits",
        evidence: "duplicate Vector quick-fix suppression",
    },
    FixtureSpec {
        name: "import_candidate_inserts_selected_function_from_provider_summary",
        category: "package metadata checks",
        evidence: "public provider-summary selected function import",
    },
    FixtureSpec {
        name: "import_candidate_rejects_private_provider_function",
        category: "rejected edits",
        evidence: "private provider function rejection",
    },
    FixtureSpec {
        name: "import_candidate_rejects_stale_reexport_provider_summary",
        category: "stale-cache rejection cases",
        evidence: "missing original re-export provider rejection",
    },
    FixtureSpec {
        name: "import_candidate_inserts_selected_function_from_generated_summary",
        category: "package metadata checks",
        evidence: "generated typi callable binding import",
    },
    FixtureSpec {
        name: "diagnostic_import_action_repairs_provider_function",
        category: "applied edits",
        evidence: "unknown-function diagnostic workspace edit",
    },
    FixtureSpec {
        name: "import_candidate_groups_selected_function_with_existing_import",
        category: "formatter parity",
        evidence: "selected import grouping without duplicate declaration",
    },
    FixtureSpec {
        name: "import_candidate_inserts_selected_constructor_from_provider_summary",
        category: "package metadata checks",
        evidence: "constructor selected import from provider summary",
    },
    FixtureSpec {
        name: "diagnostic_import_action_contains_provider_constructor_workspace_edit",
        category: "applied edits",
        evidence: "provider constructor diagnostic workspace edit",
    },
    FixtureSpec {
        name: "diagnostic_import_action_repairs_provider_constructor_pattern",
        category: "applied edits",
        evidence: "constructor-pattern diagnostic workspace edit",
    },
    FixtureSpec {
        name: "diagnostic_import_action_inserts_std_constructor_selected_fallbacks",
        category: "package metadata checks",
        evidence: "std constructor selected fallback imports",
    },
    FixtureSpec {
        name: "import_candidate_skips_already_selected_function_import",
        category: "rejected edits",
        evidence: "already selected public function suppression",
    },
    FixtureSpec {
        name: "import_candidate_skips_function_import_when_wildcard_selected",
        category: "rejected edits",
        evidence: "wildcard-selected public function suppression",
    },
    FixtureSpec {
        name: "import_candidate_keeps_source_name_candidate_when_existing_import_uses_alias",
        category: "applied edits",
        evidence: "aliased selected import keeps source-name quick fix",
    },
    FixtureSpec {
        name: "import_candidate_keeps_ambiguous_function_choices",
        category: "ambiguity rankings",
        evidence: "ambiguous public functions produce one choice per provider",
    },
];

/// Summary produced by the editor code-action auto-import report gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCodeActionAutoImportReportSummary {
    pub fixture_count: usize,
    pub category_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct FixtureSpec {
    name: &'static str,
    category: &'static str,
    evidence: &'static str,
}

#[derive(Debug, Serialize)]
struct EditorCodeActionAutoImportReport {
    gate: &'static str,
    report_schema: &'static str,
    code_action_fixtures: Vec<&'static str>,
    applied_edits: Vec<&'static str>,
    rejected_edits: Vec<&'static str>,
    ambiguity_rankings: Vec<&'static str>,
    formatter_parity: Vec<&'static str>,
    package_metadata_checks: Vec<&'static str>,
    stale_cache_rejection_cases: Vec<&'static str>,
}

/// Runs the editor code-action auto-import report gate.
///
/// Inputs:
/// - `root`: golden repository root containing `Makefile`, editor gates, and
///   LSP import-action tests.
///
/// Output:
/// - A summary with fixture/category counts and the written report path.
/// - An error when the Make target or required fixtures drift.
///
/// Transformation:
/// - Validates the editor auto-import gate and writes a JSON report describing
///   the executable code-action coverage currently owned by the LSP suite.
pub fn run_editor_code_action_auto_import_report(
    root: &Path,
) -> QualityResult<EditorCodeActionAutoImportReportSummary> {
    let makefile = read_make_graph(root)?;
    let fixtures = read_text(root, IMPORT_ACTION_TEST_PATH)?;
    let mut diagnostics = validate_gate_and_fixtures(&makefile, &fixtures);
    diagnostics.extend(validate_fixture_inventory(REQUIRED_FIXTURES));
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "editor-code-action-auto-import-report",
            &diagnostics,
        ));
    }

    let report = build_report();
    diagnostics.extend(validate_report_inventory(&report));
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "editor-code-action-auto-import-report",
            &diagnostics,
        ));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path, &report)?;

    Ok(EditorCodeActionAutoImportReportSummary {
        fixture_count: REQUIRED_FIXTURES.len(),
        category_count: report_category_count(&report),
        report_path,
    })
}

fn build_report() -> EditorCodeActionAutoImportReport {
    EditorCodeActionAutoImportReport {
        gate: TARGET,
        report_schema: "editor-code-action-auto-import-report-v1",
        code_action_fixtures: fixture_names(),
        applied_edits: evidence_for("applied edits"),
        rejected_edits: evidence_for("rejected edits"),
        ambiguity_rankings: evidence_for("ambiguity rankings"),
        formatter_parity: evidence_for("formatter parity"),
        package_metadata_checks: evidence_for("package metadata checks"),
        stale_cache_rejection_cases: evidence_for("stale-cache rejection cases"),
    }
}

fn validate_fixture_inventory(fixtures: &[FixtureSpec]) -> Vec<String> {
    if fixtures.len() == EXPECTED_FIXTURE_COUNT {
        Vec::new()
    } else {
        vec![format!(
            "`{TARGET}` must keep {EXPECTED_FIXTURE_COUNT} exact auto-import fixtures; found {}",
            fixtures.len()
        )]
    }
}

fn validate_report_inventory(report: &EditorCodeActionAutoImportReport) -> Vec<String> {
    let category_count = report_category_count(report);
    if category_count == EXPECTED_REPORT_CATEGORY_COUNT {
        Vec::new()
    } else {
        vec![format!(
            "`{TARGET}` report must keep {EXPECTED_REPORT_CATEGORY_COUNT} populated evidence categories; found {category_count}"
        )]
    }
}

fn fixture_names() -> Vec<&'static str> {
    REQUIRED_FIXTURES
        .iter()
        .map(|fixture| fixture.name)
        .collect()
}

fn evidence_for(category: &str) -> Vec<&'static str> {
    REQUIRED_FIXTURES
        .iter()
        .filter(|fixture| fixture.category == category)
        .map(|fixture| fixture.evidence)
        .collect()
}

fn report_category_count(report: &EditorCodeActionAutoImportReport) -> usize {
    [
        !report.code_action_fixtures.is_empty(),
        !report.applied_edits.is_empty(),
        !report.rejected_edits.is_empty(),
        !report.ambiguity_rankings.is_empty(),
        !report.formatter_parity.is_empty(),
        !report.package_metadata_checks.is_empty(),
        !report.stale_cache_rejection_cases.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
}

pub(crate) fn validate_gate_and_fixtures(makefile: &str, fixtures: &str) -> Vec<String> {
    let Some(body) = make_target_body(makefile, TARGET) else {
        return vec![format!("Makefile: missing target `{TARGET}`")];
    };

    let mut diagnostics = Vec::new();
    if !body.contains("editor-code-action-auto-import-report") {
        diagnostics.push(format!(
            "Makefile: `{TARGET}` must run its auto-import report"
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
    for fixture in REQUIRED_FIXTURES {
        if !fixtures.contains(&format!("fn {}", fixture.name)) {
            diagnostics.push(format!(
                "{IMPORT_ACTION_TEST_PATH}: missing required fixture `{}`",
                fixture.name
            ));
        }
    }
    diagnostics
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for fixture in REQUIRED_FIXTURES {
        diagnostics.extend(validate_entries_for_placeholder_terms(
            "fixture names",
            &[fixture.name],
        ));
        diagnostics.extend(validate_entries_for_placeholder_terms(
            &format!("fixture `{}` evidence", fixture.name),
            &[fixture.evidence],
        ));
    }
    diagnostics
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "editor code-action auto-import {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
        })
        .collect()
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

fn make_target_body(makefile: &str, target: &str) -> Option<String> {
    let target_prefix = format!("{target}:");
    let mut lines = makefile.lines();
    for line in lines.by_ref() {
        if line.trim_end().starts_with(&target_prefix) {
            break;
        }
    }
    let body = lines
        .take_while(|line| {
            line.starts_with('\t') || line.trim().is_empty() || line.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

fn write_report(path: &Path, report: &EditorCodeActionAutoImportReport) -> QualityResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(report)
        .map_err(|err| format!("{}: failed to serialize report: {err}", path.display()))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|err| format!("{}: failed to write report: {err}", path.display()))
}

#[cfg(test)]
#[path = "editor_code_action_auto_import_report_test.rs"]
mod editor_code_action_auto_import_report_test;
