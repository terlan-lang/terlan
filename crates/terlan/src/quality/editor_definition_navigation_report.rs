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

const REPORT_PATH: &str = "target/quality/editor-definition-navigation-report.json";
const TARGET: &str = "editor-definition-navigation-check";
const LSP_TEST_PATH: &str = "crates/terlan/src/lsp/lib_test.rs";
const EXPECTED_SELECTOR_COUNT: usize = 39;
const EXPECTED_REPORT_CATEGORY_COUNT: usize = 9;
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_SELECTORS: &[SelectorSpec] = &[
    SelectorSpec {
        fixture: "definition_request_returns_same_document_location",
        category: "same-document symbols",
        evidence: &[
            "function definition protocol target",
            "same-document definition request",
        ],
    },
    SelectorSpec {
        fixture: "declaration_request_returns_same_document_location",
        category: "protocol capabilities",
        evidence: &[
            "declaration-provider capability",
            "same-document declaration request",
        ],
    },
    SelectorSpec {
        fixture: "declaration_request_returns_provider_location_for_imported_reference",
        category: "imported provider symbols",
        evidence: &[
            "imported declaration request",
            "provider-summary function target",
        ],
    },
    SelectorSpec {
        fixture:
            "declaration_request_returns_provider_location_for_imported_struct_field_reference",
        category: "imported provider symbols",
        evidence: &[
            "imported field declaration request",
            "provider-summary field target",
        ],
    },
    SelectorSpec {
        fixture: "type_definition_request_returns_same_document_type_location",
        category: "protocol capabilities",
        evidence: &[
            "type-definition provider capability",
            "same-document type annotation target",
        ],
    },
    SelectorSpec {
        fixture: "type_definition_request_returns_provider_location_for_imported_type_annotation",
        category: "imported provider symbols",
        evidence: &[
            "imported type-definition request",
            "provider-summary type target",
        ],
    },
    SelectorSpec {
        fixture: "implementation_request_returns_same_document_impl_method_location",
        category: "protocol capabilities",
        evidence: &["implementation-provider capability", "impl method target"],
    },
    SelectorSpec {
        fixture: "references_request_excludes_trait_and_impl_method_declarations",
        category: "references fixtures",
        evidence: &[
            "declaration exclusion",
            "trait and impl reference filtering",
        ],
    },
    SelectorSpec {
        fixture: "references_request_returns_same_document_identifier_locations",
        category: "references fixtures",
        evidence: &[
            "same-document references",
            "identifier-token reference scan",
        ],
    },
    SelectorSpec {
        fixture: "references_request_preserves_imported_use_site_without_declaration",
        category: "references fixtures",
        evidence: &[
            "imported use-site reference",
            "declaration exclusion preservation",
        ],
    },
    SelectorSpec {
        fixture: "definition_request_returns_provider_location_for_imported_reference",
        category: "imported provider symbols",
        evidence: &[
            "imported definition request",
            "provider-summary function target",
        ],
    },
    SelectorSpec {
        fixture: "definition_request_returns_provider_location_for_imported_struct_field_reference",
        category: "imported provider symbols",
        evidence: &[
            "imported struct-field definition request",
            "provider-summary field target",
        ],
    },
    SelectorSpec {
        fixture: "definition_request_returns_provider_location_for_imported_shape_reference",
        category: "imported provider symbols",
        evidence: &[
            "imported shape definition request",
            "provider-summary shape target",
        ],
    },
    SelectorSpec {
        fixture: "definition_request_returns_provider_location_for_imported_constructor_reference",
        category: "imported provider symbols",
        evidence: &[
            "imported constructor definition request",
            "provider-summary constructor target",
        ],
    },
    SelectorSpec {
        fixture: "definition_request_returns_provider_location_for_imported_trait_reference",
        category: "imported provider symbols",
        evidence: &[
            "imported trait definition request",
            "provider-summary trait target",
        ],
    },
    SelectorSpec {
        fixture: "definition_request_returns_empty_for_template_documents",
        category: "template non-navigation",
        evidence: &[
            "template definition request rejection",
            "no stale Terlan source target",
        ],
    },
    SelectorSpec {
        fixture: "references_request_returns_empty_for_template_documents",
        category: "template non-navigation",
        evidence: &[
            "template references request rejection",
            "no template source reference leak",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_same_document_function",
        category: "same-document symbols",
        evidence: &[
            "helper-level local function definition",
            "source span target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_use_formatter_shifted_function_spans",
        category: "formatter span shift fixtures",
        evidence: &[
            "formatter-shifted function span",
            "post-format source offset target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_same_document_type_annotation",
        category: "same-document symbols",
        evidence: &["same-document type annotation target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_same_document_impl_method_reference",
        category: "same-document symbols",
        evidence: &["same-document impl method target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_same_document_field_reference",
        category: "same-document symbols",
        evidence: &["same-document field target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_wildcard_selected_imported_function",
        category: "imported provider symbols",
        evidence: &[
            "wildcard selected import",
            "provider-summary function target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_selected_import_alias_to_provider",
        category: "imported provider symbols",
        evidence: &["selected import alias", "provider declaration target"],
    },
    SelectorSpec {
        fixture: "definition_locations_reject_ambiguous_imported_symbol",
        category: "stale metadata rejections",
        evidence: &[
            "ambiguous imported symbol rejection",
            "no arbitrary provider target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_reject_private_provider_symbol",
        category: "stale metadata rejections",
        evidence: &[
            "private provider symbol rejection",
            "public interface boundary",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_reject_missing_provider_file",
        category: "stale metadata rejections",
        evidence: &["missing provider file rejection", "no stale file target"],
    },
    SelectorSpec {
        fixture: "definition_locations_prefer_local_definition_over_import",
        category: "same-document symbols",
        evidence: &["local definition precedence", "import shadowing avoidance"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_nested_package_provider_function",
        category: "imported provider symbols",
        evidence: &[
            "nested package provider summary",
            "dotted import function target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_imported_overloaded_function",
        category: "imported provider symbols",
        evidence: &[
            "imported overload summary",
            "selected imported overload target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_follow_selected_reexport_summary",
        category: "imported provider symbols",
        evidence: &[
            "selected re-export summary",
            "original provider declaration target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_reject_stale_reexport_provider_summary",
        category: "stale metadata rejections",
        evidence: &[
            "stale re-export rejection",
            "renamed provider artifact rejection",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_generated_summary_function_binding",
        category: "generated binding fixtures",
        evidence: &[
            "generated summary function binding",
            "packaged-summary fallback target",
        ],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_generated_std_summary_type",
        category: "generated binding fixtures",
        evidence: &["generated std summary type", "packaged-summary type target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_imported_type_annotation",
        category: "imported provider symbols",
        evidence: &["imported type annotation helper target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_imported_struct_field_reference",
        category: "imported provider symbols",
        evidence: &["imported struct-field helper target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_imported_shape_reference",
        category: "imported provider symbols",
        evidence: &["imported shape helper target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_imported_constructor_reference",
        category: "imported provider symbols",
        evidence: &["imported constructor helper target"],
    },
    SelectorSpec {
        fixture: "definition_locations_resolve_imported_trait_reference",
        category: "imported provider symbols",
        evidence: &["imported trait helper target"],
    },
];

const REPORT_NOTES: &[&str] = &[
    "VS Code shift-click uses the same LSP definition/declaration/type-definition protocol path.",
    "Neovim, Emacs, and IntelliJ integrations consume the same language-server navigation contract.",
    "Template documents intentionally return no Terlan source navigation target until typed template source maps are owned by the template slice.",
];

/// Summary produced by the editor definition-navigation report gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorDefinitionNavigationReportSummary {
    pub selector_count: usize,
    pub category_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct EditorDefinitionNavigationReport {
    gate: &'static str,
    report_schema: &'static str,
    same_document_symbols: Vec<&'static str>,
    imported_provider_symbols: Vec<&'static str>,
    protocol_capabilities: Vec<&'static str>,
    references_fixtures: Vec<&'static str>,
    stale_metadata_rejections: Vec<&'static str>,
    generated_binding_fixtures: Vec<&'static str>,
    template_non_navigation: Vec<&'static str>,
    formatter_span_shift_fixtures: Vec<&'static str>,
    editor_parity_notes: Vec<&'static str>,
}

/// Runs the editor definition-navigation report gate.
///
/// Inputs:
/// - `root`: golden repository root containing `Makefile` and editor gates.
///
/// Output:
/// - A summary with selector/category counts and the written report path.
/// - An error when the Make target or required exact selectors drift.
///
/// Transformation:
/// - Reads the Make include graph, validates the editor definition-navigation
///   gate body, and writes a JSON report mirroring executable LSP evidence.
pub fn run_editor_definition_navigation_report(
    root: &Path,
) -> QualityResult<EditorDefinitionNavigationReportSummary> {
    let makefile = read_make_graph(root)?;
    let fixtures = read_text(root, LSP_TEST_PATH)?;
    let mut diagnostics = validate_gate_and_fixtures(&makefile, &fixtures);
    diagnostics.extend(validate_selector_inventory(REQUIRED_SELECTORS));
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "editor-definition-navigation-report",
            &diagnostics,
        ));
    }

    let report = build_report();
    diagnostics.extend(validate_report_inventory(&report));
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "editor-definition-navigation-report",
            &diagnostics,
        ));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path, &report)?;

    Ok(EditorDefinitionNavigationReportSummary {
        selector_count: REQUIRED_SELECTORS.len(),
        category_count: report_category_count(&report),
        report_path,
    })
}

fn build_report() -> EditorDefinitionNavigationReport {
    EditorDefinitionNavigationReport {
        gate: TARGET,
        report_schema: "editor-definition-navigation-report-v1",
        same_document_symbols: fixtures_for("same-document symbols"),
        imported_provider_symbols: fixtures_for("imported provider symbols"),
        protocol_capabilities: fixtures_for("protocol capabilities"),
        references_fixtures: fixtures_for("references fixtures"),
        stale_metadata_rejections: fixtures_for("stale metadata rejections"),
        generated_binding_fixtures: fixtures_for("generated binding fixtures"),
        template_non_navigation: fixtures_for("template non-navigation"),
        formatter_span_shift_fixtures: fixtures_for("formatter span shift fixtures"),
        editor_parity_notes: REPORT_NOTES.to_vec(),
    }
}

fn validate_selector_inventory(selectors: &[SelectorSpec]) -> Vec<String> {
    if selectors.len() == EXPECTED_SELECTOR_COUNT {
        Vec::new()
    } else {
        vec![format!(
            "`{TARGET}` must keep {EXPECTED_SELECTOR_COUNT} exact navigation selectors; found {}",
            selectors.len()
        )]
    }
}

fn validate_report_inventory(report: &EditorDefinitionNavigationReport) -> Vec<String> {
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

fn report_category_count(report: &EditorDefinitionNavigationReport) -> usize {
    [
        !report.same_document_symbols.is_empty(),
        !report.imported_provider_symbols.is_empty(),
        !report.protocol_capabilities.is_empty(),
        !report.references_fixtures.is_empty(),
        !report.stale_metadata_rejections.is_empty(),
        !report.generated_binding_fixtures.is_empty(),
        !report.template_non_navigation.is_empty(),
        !report.formatter_span_shift_fixtures.is_empty(),
        !report.editor_parity_notes.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
}

/// Validates canonical suite ownership, report wiring, and LSP fixture presence.
pub(crate) fn validate_gate_and_fixtures(makefile: &str, fixtures: &str) -> Vec<String> {
    let Some(body) = make_target_body(makefile, TARGET) else {
        return vec![format!("Makefile: missing target `{TARGET}`")];
    };

    let mut diagnostics = Vec::new();
    if !body.contains("editor-definition-navigation-report") {
        diagnostics.push(format!(
            "Makefile: `{TARGET}` must run its definition-navigation report"
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
                "{LSP_TEST_PATH}: missing navigation fixture `{}`",
                selector.fixture
            ));
        }
    }
    diagnostics
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    selector_evidence_placeholder_diagnostics(
        REQUIRED_SELECTORS
            .iter()
            .map(|selector| (selector.fixture, selector.evidence)),
        REPORT_NOTES,
        validate_entries_for_placeholder_terms,
    )
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    placeholder_entry_diagnostics(
        label,
        entries,
        PLACEHOLDER_REPORT_TERMS,
        |label, entry, term| {
            format!(
            "editor definition-navigation {label} entry `{entry}` uses placeholder term `{term}`"
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

fn write_report(path: &Path, report: &EditorDefinitionNavigationReport) -> QualityResult<()> {
    write_json_report(path, report)
}

#[cfg(test)]
#[path = "editor_definition_navigation_report_test.rs"]
#[cfg(test)]
mod editor_definition_navigation_report_test;
