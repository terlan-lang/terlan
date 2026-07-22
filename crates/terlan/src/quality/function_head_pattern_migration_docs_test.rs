use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_migration_doc() -> String {
    let mut text = MIGRATION_DOC_REQUIRED_TERMS.join("\n");
    for migration_id in MIGRATION_IDS {
        text.push_str("\n## ");
        text.push_str(migration_id);
        text.push('\n');
        text.push_str(migration_id);
        text.push('\n');
    }
    text
}

fn complete_text(terms: &[&str]) -> String {
    let mut text = terms.join("\n");
    for migration_id in MIGRATION_IDS {
        text.push('\n');
        text.push_str(migration_id);
    }
    text
}

/// Verifies the function-head pattern migration docs gate writes the
/// roadmap-required closeout report.
#[test]
fn function_head_pattern_migration_docs_writes_report() {
    let repo = TempRepo::new("function_head_pattern_migration_docs_writes_report");
    repo.write_root(FUNCTION_HEADS_DOC, &complete_migration_doc());
    repo.write_root(README_DOC, &complete_text(README_REQUIRED_TERMS));
    repo.write_workspace(
        "docs/roadmap/README.md",
        &complete_text(ROADMAP_README_REQUIRED_TERMS),
    );
    repo.write_workspace(
        "docs/roadmap/RELEASE_NOTES_0_0_7.md",
        &complete_text(RELEASE_NOTES_REQUIRED_TERMS),
    );

    let summary = run_function_head_pattern_migration_docs(repo.root())
        .expect("function-head pattern migration docs gate");

    assert_eq!(MIGRATION_IDS.len(), summary.migration_id_count);
    assert_eq!(
        MIGRATION_DOC_REQUIRED_TERMS.len()
            + README_REQUIRED_TERMS.len()
            + ROADMAP_README_REQUIRED_TERMS.len()
            + RELEASE_NOTES_REQUIRED_TERMS.len(),
        summary.required_term_count
    );
    assert_eq!(
        "target/quality/function-head-pattern-migration-docs-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.root()
            .join("target/quality/function-head-pattern-migration-docs-report.json"),
    )
    .expect("read function-head pattern migration docs report");
    assert!(report.contains("terlan.function-head-pattern-migration-docs.v1"));
    assert!(report.contains("diagnostic_doc_ids"));
    assert!(report.contains("migration.function_head_pattern.remains"));
}

/// Verifies missing migration guide anchors fail the closeout gate.
#[test]
fn function_head_pattern_migration_docs_rejects_missing_anchor() {
    let mut migration_doc = MIGRATION_DOC_REQUIRED_TERMS.join("\n");
    migration_doc.push_str("\nmigration.function_head_pattern.invalid_alias_style\n");
    migration_doc.push_str("\nmigration.function_head_pattern.safe_reject\n");
    migration_doc.push_str("\nmigration.function_head_pattern.unsupported_backend\n");
    migration_doc.push_str("\nmigration.function_head_pattern.remains\n");

    let diagnostics = validate_function_head_pattern_migration_docs_texts(
        &migration_doc,
        &complete_text(README_REQUIRED_TERMS),
        &complete_text(ROADMAP_README_REQUIRED_TERMS),
        &complete_text(RELEASE_NOTES_REQUIRED_TERMS),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing migration guide anchor heading")),
        "diagnostics should reject missing anchors: {diagnostics:?}"
    );
}

/// Verifies weakening claims are rejected.
#[test]
fn function_head_pattern_migration_docs_rejects_optional_doc_links() {
    let diagnostics = validate_function_head_pattern_migration_docs_texts(
        &complete_migration_doc(),
        &complete_text(README_REQUIRED_TERMS),
        &complete_text(ROADMAP_README_REQUIRED_TERMS),
        &format!(
            "{}\ndocs links are optional for migration diagnostics",
            complete_text(RELEASE_NOTES_REQUIRED_TERMS)
        ),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("docs links are optional")),
        "diagnostics should reject optional docs link claims: {diagnostics:?}"
    );
}

/// Verifies placeholder closeout text is rejected.
#[test]
fn function_head_pattern_migration_docs_rejects_placeholder_text() {
    let diagnostics = validate_function_head_pattern_migration_docs_texts(
        &format!("{}\nTODO: document this later", complete_migration_doc()),
        &complete_text(README_REQUIRED_TERMS),
        &complete_text(ROADMAP_README_REQUIRED_TERMS),
        &complete_text(RELEASE_NOTES_REQUIRED_TERMS),
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .contains("placeholder function-head pattern migration docs text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

struct TempRepo {
    workspace: PathBuf,
    root: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!("terlan_{name}_{stamp}"));
        let root = workspace.join("terlan");
        fs::create_dir_all(&root).expect("create temp repo");
        Self { workspace, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write_root(&self, relative_path: &str, text: &str) {
        write_file(&self.root.join(relative_path), text);
    }

    fn write_workspace(&self, relative_path: &str, text: &str) {
        write_file(&self.workspace.join(relative_path), text);
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.workspace);
    }
}

fn write_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, text).expect("write fixture");
}
