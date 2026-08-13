use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/typed-template-render-mode-report.json";

const REQUIRED_STRUCTURE_ANCHORS: &[&str] = &[
    "validate_artifact_template_structure",
    "ArtifactTemplateTarget::Html",
    "ArtifactTemplateTarget::Markdown",
    "ArtifactTemplateTarget::Json",
    "ArtifactTemplateTarget::Toml",
    "ArtifactTemplateTarget::Yaml",
    "ArtifactTemplateTarget::Text",
    "ArtifactTemplateTarget::Xml",
    "validate_xml_template_structure",
];

const REQUIRED_ESCAPING_ANCHORS: &[&str] = &[
    "escape_html_attr",
    "escape_html_text",
    "ammonia::clean_text",
];

const REQUIRED_TEMPLATE_CONTRACT_ANCHORS: &[&str] = &[
    "Template.Html",
    "template_slot_typecheck_rejects_html_fragment_in_attribute_context",
    "template_slot_typecheck_accepts_scalar_struct_field_in_text_context",
    "template_component_prop_accepts_expression_slot_matching_expected_type",
];

const REQUIRED_SOURCE_ANCHORS: &[&str] = &[
    "pub opaque type Html",
    "pub trusted(value: String): Html",
    "pub html(value: std.template.Template.Html",
    "Response.html(page())",
    "format_template_decl",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "typed-template-render-mode-check: vm-live-template-client-protocol-check typed-template-interpolation-check",
    "node editors/vscode/test/template_links_test.js",
    "typed_template_render_mode_test",
    "typed-template-render-mode",
];

const REQUIRED_DOC_EDITOR_ANCHORS: &[(&str, &[&str], &str)] = &[
    (
        "crates/terlan/src/commands/doc/README.md",
        &[
            "Render Mode Parity",
            "staticHtml",
            "documentationExample",
            "structuredArtifact",
        ],
        "generated documentation render-mode parity",
    ),
    (
        "editors/vscode/src/template_links.js",
        &[
            "templateRenderModeFromPath",
            "documentationExample",
            "structuredArtifact",
        ],
        "editor template render-mode inference",
    ),
    (
        "editors/vscode/test/template_links_test.js",
        &[
            "testTemplateRenderModeFromPath",
            "templateRenderModeFromPath(\"templates/readme.terl.md\")",
            "renderMode",
        ],
        "editor template render-mode tests",
    ),
];

const RENDER_MODES: &[(&str, bool, &str)] = &[
    (
        "staticHtml",
        true,
        "static site Template.Html route rendering",
    ),
    (
        "serverRenderedHtml",
        true,
        "Response.html Template.Html descriptors",
    ),
    (
        "structuredArtifact",
        true,
        "HTML/JSON/YAML/TOML/XML/text artifact validation",
    ),
    (
        "streamingHtml",
        false,
        "streaming fragment budgets remain rejected",
    ),
    (
        "liveDomPatch",
        false,
        "live DOM patch mode remains rejected",
    ),
    (
        "clientHydratedFragment",
        false,
        "client hydration compatibility remains rejected",
    ),
    (
        "emailSafeMarkup",
        false,
        "email-safe escaping policy remains rejected",
    ),
    (
        "documentationExample",
        true,
        "docs/editor render-mode parity for documentation examples",
    ),
];

const ESCAPING_CHECKS: &[&str] = &[
    "HTML attribute escaping",
    "HTML text escaping through maintained sanitizer",
    "Template.Html trusted-fragment separation",
    "attribute context rejects Template.Html",
];

const PERFORMANCE_BUDGETS: &[&str] = &[
    "staticHtml.maxRenderMs=5",
    "serverRenderedHtml.maxDescriptorBytes=4096",
    "structuredArtifact.maxValidationMs=10",
    "streamingHtml.rejectedUntilBackpressureBudgetExists",
    "liveDomPatch.rejectedUntilPatchByteBudgetExists",
    "clientHydratedFragment.rejectedUntilHydrationMismatchBudgetExists",
];

const PLACEHOLDER_BUDGET_TERMS: &[&str] = &["placeholder", "todo", "tbd", "unknown"];

const REJECTED_MODE_COMBINATIONS: &[&str] = &[
    "wrong escaping mode",
    "cross-mode component reuse",
    "stale asset hash",
    "oversized live patch",
    "slow streaming fragment",
    "actor binding in static mode",
    "hydration mismatch",
    "email-safe markup with live actor subscription",
];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing typed template render mode summary.
pub struct TypedTemplateRenderModeSummary {
    pub render_mode_count: usize,
    pub implemented_mode_count: usize,
    pub escaping_check_count: usize,
    pub rejected_mode_combination_count: usize,
    pub report_path: PathBuf,
}

/// Runs typed template render mode.
pub fn run_typed_template_render_mode(
    root: &Path,
) -> QualityResult<TypedTemplateRenderModeSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/html/structured.rs",
        REQUIRED_STRUCTURE_ANCHORS,
        "typed template structure validation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/html/escaping.rs",
        REQUIRED_ESCAPING_ANCHORS,
        "typed template escaping",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/validation/template_contract/template_contract_test.rs",
        REQUIRED_TEMPLATE_CONTRACT_ANCHORS,
        "template contract adversarial coverage",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/template/Template.terl",
        &REQUIRED_SOURCE_ANCHORS[..2],
        "std template source surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/Response.terl",
        &REQUIRED_SOURCE_ANCHORS[2..3],
        "HTTP template response surface",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/compiler/typeck/std_contract_test.rs",
        &REQUIRED_SOURCE_ANCHORS[3..4],
        "Response.html Template.Html contract test",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/compiler/syntax/formatter/metadata.rs",
        &REQUIRED_SOURCE_ANCHORS[4..],
        "template formatter visibility",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    for (relative, anchors, label) in REQUIRED_DOC_EDITOR_ANCHORS {
        diagnostics.extend(validate_required_terms(root, relative, anchors, label)?);
    }
    diagnostics.extend(validate_performance_budgets());
    if !diagnostics.is_empty() {
        return Err(render_failure("typed-template-render-mode", &diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let implemented_mode_count = RENDER_MODES
        .iter()
        .filter(|(_, implemented, _)| *implemented)
        .count();
    let render_modes: Vec<_> = RENDER_MODES
        .iter()
        .map(|(name, implemented, evidence)| {
            json!({
                "name": name,
                "implemented": implemented,
                "evidence": evidence
            })
        })
        .collect();
    let report = json!({
        "schema": "terlan-typed-template-render-mode-report-v1",
        "templateInventory": render_modes,
        "inferredModes": [
            "staticHtml from Template.Html static route output",
            "serverRenderedHtml from Response.html Template.Html descriptors",
            "structuredArtifact from .terl.json/.terl.yaml/.terl.toml/.terl.txt suffixes",
            "documentationExample from .terl.md editor/documentation templates"
        ],
        "explicitModes": {
            "implemented": false,
            "reason": "explicit source-level render mode declarations remain rejected for this slice"
        },
        "escapingChecks": ESCAPING_CHECKS,
        "performanceBudgets": PERFORMANCE_BUDGETS,
        "assetHashes": {
            "implemented": false,
            "reason": "asset hash enforcement belongs to the following web asset pipeline slice"
        },
        "sourceMapParity": {
            "implemented": false,
            "reason": "VM/JS/Wasm/docs/editor render-mode parity remains rejected for this slice"
        },
        "docsEditorParity": {
            "implemented": true,
            "evidence": [
                "doc README declares stable render-mode names",
                "VS Code template links infer render modes from template suffixes",
                "editor parser tests assert renderMode metadata"
            ]
        },
        "rejectedModeCombinations": REJECTED_MODE_COMBINATIONS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize typed template render mode report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(TypedTemplateRenderModeSummary {
        render_mode_count: RENDER_MODES.len(),
        implemented_mode_count,
        escaping_check_count: ESCAPING_CHECKS.len(),
        rejected_mode_combination_count: REJECTED_MODE_COMBINATIONS.len(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read {label}: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile")).map_err(|err| {
        format!("Makefile: failed to read typed template render mode gate: {err}")
    })?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing typed template render mode gate term `{term}`"))
        .collect())
}

fn validate_performance_budgets() -> Vec<String> {
    validate_performance_budget_terms(PERFORMANCE_BUDGETS)
}

fn validate_performance_budget_terms(budgets: &[&str]) -> Vec<String> {
    budgets
        .iter()
        .filter_map(|budget| {
            let normalized = budget.to_ascii_lowercase();
            let has_placeholder = PLACEHOLDER_BUDGET_TERMS
                .iter()
                .any(|term| normalized.contains(term));
            if has_placeholder {
                return Some(format!(
                    "typed template render mode budget `{budget}` uses placeholder language"
                ));
            }
            let is_positive_budget = budget.contains(".max") && budget.contains('=');
            let is_explicit_rejection = budget.contains(".rejectedUntil");
            if is_positive_budget || is_explicit_rejection {
                None
            } else {
                Some(format!(
                    "typed template render mode budget `{budget}` must be a max* threshold or rejectedUntil reason"
                ))
            }
        })
        .collect()
}

fn render_failure(label: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{label}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "typed_template_render_mode_test.rs"]
#[cfg(test)]
mod typed_template_render_mode_test;
