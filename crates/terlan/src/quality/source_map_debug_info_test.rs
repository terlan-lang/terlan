use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the source-map/debug-info gate writes the roadmap-required report.
#[test]
fn source_map_debug_info_writes_report() {
    let repo = TempRepo::new("source_map_debug_info_writes_report");
    repo.write(
        "docs/compiler/SOURCE_MAP_DEBUG_INFO.md",
        &complete_contract(),
    );

    let summary = run_source_map_debug_info(repo.path()).expect("source-map/debug-info gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/source-map-debug-info-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/source-map-debug-info-report.json"),
    )
    .expect("read source-map/debug-info report");
    assert!(report.contains("terlan.source-map-debug-info.v1"));
    assert!(report.contains("source-map/debug-info contract"));
    assert!(report.contains("span_roundtrips"));
}

/// Verifies source checkout path dependency claims are rejected.
#[test]
fn source_map_debug_info_rejects_source_checkout_dependency_claims() {
    let text = format!(
        "{}\nsource maps may depend on source checkout paths",
        complete_contract()
    );

    let diagnostics = validate_source_map_debug_info_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("source checkout paths")),
        "diagnostics should reject source checkout dependency claims: {diagnostics:?}"
    );
}

/// Verifies span roundtrip evidence is required.
#[test]
fn source_map_debug_info_rejects_missing_span_roundtrips() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "span roundtrips")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_source_map_debug_info_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("span roundtrips")),
        "diagnostics should reject missing span roundtrip evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder source-map/debug-info contracts are rejected.
#[test]
fn source_map_debug_info_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define source maps later", complete_contract());

    let diagnostics = validate_source_map_debug_info_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder source-map/debug-info text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("terlan_{name}_{stamp}"));
        fs::create_dir_all(&path).expect("create temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, text: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, text).expect("write fixture");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
