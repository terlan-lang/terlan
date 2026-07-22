use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_GATES
        .iter()
        .chain(CLOSURE_MATRIX_ROWS)
        .chain(PERMANENT_BEHAVIOR_TERMS)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Verifies the handoff gate writes the requested release manifest.
#[test]
fn function_head_pattern_handoff_writes_manifest() {
    let repo = TempRepo::new("function_head_pattern_handoff_writes_manifest");
    repo.write_root(MIGRATION_POLICY_DOC, &complete_contract());
    repo.write_root(MIGRATION_GUIDE_DOC, &complete_contract());
    repo.write_workspace(
        "docs/roadmap/ROADMAP_0_0_7.md",
        &format!("{}\nfeature from in progress\n", complete_contract()),
    );
    repo.write_workspace(
        "docs/roadmap/RELEASE_NOTES_0_0_7.md",
        &format!(
            "{}\nfunction-head pattern migration closeout docs\n",
            complete_contract()
        ),
    );

    let summary =
        run_function_head_pattern_handoff(repo.root()).expect("function-head handoff gate");

    assert_eq!(REQUIRED_GATES.len(), summary.required_gate_count);
    assert_eq!(CLOSURE_MATRIX_ROWS.len(), summary.closure_matrix_row_count);
    assert_eq!(
        "target/quality/function-head-pattern-handoff-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(repo.root().join(&summary.report_path))
        .expect("read function-head handoff report");
    assert!(report.contains("terlan.function-head-pattern-handoff.v1"));
    assert!(report.contains("function-head-pattern-0-0-7-handoff-check"));
    assert!(report.contains("timing_snapshot"));
}

/// Verifies missing closure matrix rows fail the handoff gate.
#[test]
fn function_head_pattern_handoff_rejects_missing_matrix_row() {
    let incomplete = complete_contract().replace("migration benchmark", "");

    let diagnostics = validate_function_head_pattern_handoff_texts(
        &incomplete,
        &incomplete,
        &format!("{incomplete}\nfeature from in progress\n"),
        &format!("{incomplete}\nfunction-head pattern migration closeout docs\n"),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("migration benchmark")),
        "expected missing matrix row diagnostic: {diagnostics:?}"
    );
}

/// Verifies compatibility shims cannot be claimed as default-path behavior.
#[test]
fn function_head_pattern_handoff_rejects_default_path_shim_claim() {
    let text = format!(
        "{}\nnormal default-path codepaths use compatibility shims\n",
        complete_contract()
    );

    let diagnostics = validate_function_head_pattern_handoff_texts(
        &text,
        &text,
        &format!("{text}\nfeature from in progress\n"),
        &format!("{text}\nfunction-head pattern migration closeout docs\n"),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("compatibility shims")),
        "expected compatibility shim diagnostic: {diagnostics:?}"
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
