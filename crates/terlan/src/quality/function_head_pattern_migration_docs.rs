use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const FUNCTION_HEADS_DOC: &str = "docs/language/function_heads.md";
const README_DOC: &str = "README.md";
const ROADMAP_README_DOC: &str = "../docs/roadmap/README.md";
const RELEASE_NOTES_DOC: &str = "../docs/roadmap/RELEASE_NOTES_0_0_7.md";
const REPORT_PATH: &str = "target/quality/function-head-pattern-migration-docs-report.json";

const MIGRATION_IDS: &[&str] = &[
    "migration.function_head_pattern.invalid_alias_style",
    "migration.function_head_pattern.safe_reject",
    "migration.function_head_pattern.unsupported_backend",
    "migration.function_head_pattern.remains",
];

const MIGRATION_DOC_REQUIRED_TERMS: &[&str] = &[
    "Function Head Pattern Migration",
    "Version: 0.0.7",
    "versioned migration guide",
    "Accepted Pattern Forms",
    "Rejected Pattern Forms",
    "Strict-Mode Behavior",
    "CLI/IDE Assist Workflow",
    "Backend Fallback Caveats",
    "before/after",
    "accepted with warning",
    "parser output",
    "legacy alias order",
    "once per file in strict mode only",
    "docs link in CLI diagnostics",
    "diagnostic-to-doc-id round-trip",
    "markdown snapshot assertion",
    "changelog/release-note anchor",
    "support matrix for VM/JS targets",
    "function-head-pattern-migration-docs-check",
];

const README_REQUIRED_TERMS: &[&str] = &[
    "README quickstart migration example",
    "docs/language/function_heads.md#migrationfunction_head_patterninvalid_alias_style",
    "migration.function_head_pattern.invalid_alias_style",
];

const ROADMAP_README_REQUIRED_TERMS: &[&str] = &[
    "Function-Head Pattern Migration Deprecation Timeline",
    "0.0.7 slice completion date",
    "0.0.8",
    "support matrix for VM/JS targets",
    "accepted with warning",
    "migration.function_head_pattern.invalid_alias_style",
    "migration.function_head_pattern.safe_reject",
    "migration.function_head_pattern.unsupported_backend",
    "migration.function_head_pattern.remains",
];

const RELEASE_NOTES_REQUIRED_TERMS: &[&str] = &[
    "function-head pattern migration closeout docs",
    "function-head-pattern-migration-docs-check",
    "versioned migration guide",
    "README quickstart migration example",
    "legacy-only codebase",
    "safe automated migration output",
    "accepted with warning",
    "docs link in CLI diagnostics",
    "migration.function_head_pattern.invalid_alias_style",
    "migration.function_head_pattern.safe_reject",
    "migration.function_head_pattern.unsupported_backend",
    "migration.function_head_pattern.remains",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "migration IDs can be omitted",
    "release note entry can state completion before this gate is green",
    "legacy alias order is silently accepted",
    "docs links are optional for migration diagnostics",
    "0.0.8 removal timeline is unknown",
    "JS backend fallback is implicit",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the function-head pattern migration docs gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionHeadPatternMigrationDocsSummary {
    pub migration_id_count: usize,
    pub required_term_count: usize,
    pub report_path: String,
}

/// Runs the function-head pattern migration docs closeout gate.
///
/// Inputs:
/// - `root`: repository root containing `README.md` and `docs/language/`.
///
/// Output:
/// - Success summary and report when the migration guide, README example,
///   roadmap deprecation timeline, and release notes anchor are complete.
/// - Stable diagnostics when any migration ID lacks a docs reference.
///
/// Transformation:
/// - Converts the Slice 12 migration closeout requirements into executable
///   release evidence for 0.0.7.
pub fn run_function_head_pattern_migration_docs(
    root: &Path,
) -> QualityResult<FunctionHeadPatternMigrationDocsSummary> {
    let migration_doc = read_required_file(root, FUNCTION_HEADS_DOC)?;
    let readme = read_required_file(root, README_DOC)?;
    let roadmap_readme = read_required_file(root, ROADMAP_README_DOC)?;
    let release_notes = read_required_file(root, RELEASE_NOTES_DOC)?;

    let diagnostics = validate_function_head_pattern_migration_docs_texts(
        &migration_doc,
        &readme,
        &roadmap_readme,
        &release_notes,
    );
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(FunctionHeadPatternMigrationDocsSummary {
        migration_id_count: MIGRATION_IDS.len(),
        required_term_count: MIGRATION_DOC_REQUIRED_TERMS.len()
            + README_REQUIRED_TERMS.len()
            + ROADMAP_README_REQUIRED_TERMS.len()
            + RELEASE_NOTES_REQUIRED_TERMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn read_required_file(root: &Path, relative_path: &str) -> QualityResult<String> {
    let path = root.join(relative_path);
    fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read function-head pattern migration docs input: {err}",
            path.display()
        )
    })
}

fn validate_function_head_pattern_migration_docs_texts(
    migration_doc: &str,
    readme: &str,
    roadmap_readme: &str,
    release_notes: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    require_terms(
        "docs/language/function_heads.md",
        migration_doc,
        MIGRATION_DOC_REQUIRED_TERMS,
        &mut diagnostics,
    );
    require_terms("README.md", readme, README_REQUIRED_TERMS, &mut diagnostics);
    require_terms(
        "docs/roadmap/README.md",
        roadmap_readme,
        ROADMAP_README_REQUIRED_TERMS,
        &mut diagnostics,
    );
    require_terms(
        "docs/roadmap/RELEASE_NOTES_0_0_7.md",
        release_notes,
        RELEASE_NOTES_REQUIRED_TERMS,
        &mut diagnostics,
    );

    let combined = [migration_doc, readme, roadmap_readme, release_notes].join("\n");
    let normalized_combined = combined.to_lowercase();
    for migration_id in MIGRATION_IDS {
        let occurrences = combined.matches(migration_id).count();
        if occurrences < 4 {
            diagnostics.push(format!(
                "migration ID `{migration_id}` must appear in migration guide, README/timeline, and release notes; found {occurrences} references"
            ));
        }
        let anchor = format!("## {migration_id}");
        if !migration_doc.contains(&anchor) {
            diagnostics.push(format!("missing migration guide anchor heading `{anchor}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized_combined.contains(&claim.to_lowercase()) {
            diagnostics.push(format!(
                "forbidden function-head pattern migration docs claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized_combined.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder function-head pattern migration docs text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn require_terms(label: &str, text: &str, terms: &[&str], diagnostics: &mut Vec<String>) {
    let normalized = text.to_lowercase();
    for term in terms {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing {label} term `{term}`"));
        }
    }
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create function-head pattern migration docs report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.function-head-pattern-migration-docs.v1",
        "migration_doc": FUNCTION_HEADS_DOC,
        "roadmap_timeline": "docs/roadmap/README.md",
        "release_note_anchor": "docs/roadmap/RELEASE_NOTES_0_0_7.md",
        "readme_quickstart_example": README_DOC,
        "diagnostic_doc_ids": MIGRATION_IDS,
        "stale_syntax_lint": "migration.function_head_pattern.remains",
        "snapshot_assertions": [
            "markdown snapshot assertion",
            "CLI diagnostic-to-doc-id round-trip",
            "changelog/release-note anchor"
        ]
    });
    let text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize function-head pattern migration docs report: {err}")
    })?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write function-head pattern migration docs report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[function-head-pattern-migration-docs] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "function_head_pattern_migration_docs_test.rs"]
#[cfg(test)]
mod function_head_pattern_migration_docs_test;
