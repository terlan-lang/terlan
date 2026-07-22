use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the watch mode and VM hot-reload gate writes the
/// roadmap-required report.
#[test]
fn watch_mode_hot_reload_writes_report() {
    let repo = TempRepo::new("watch_mode_hot_reload_writes_report");
    repo.write(
        "docs/compiler/WATCH_MODE_HOT_RELOAD.md",
        &complete_contract(),
    );

    let summary = run_watch_mode_hot_reload(repo.path()).expect("watch mode hot-reload gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/watch-mode-hot-reload-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/watch-mode-hot-reload-report.json"),
    )
    .expect("read watch mode hot-reload report");
    assert!(report.contains("terlan.watch-mode-hot-reload.v1"));
    assert!(report.contains("watch mode and VM hot-reload correctness contract"));
    assert!(report.contains("event_sequences"));
}

/// Verifies mixed-code-version hot reload claims are rejected.
#[test]
fn watch_mode_hot_reload_rejects_mixed_code_version_claims() {
    let text = format!(
        "{}\nVM hot reload can expose mixed code versions",
        complete_contract()
    );

    let diagnostics = validate_watch_mode_hot_reload_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("mixed code versions")),
        "diagnostics should reject mixed-code-version claims: {diagnostics:?}"
    );
}

/// Verifies stable event evidence is required.
#[test]
fn watch_mode_hot_reload_rejects_missing_event_sequences() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "event sequences")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_watch_mode_hot_reload_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("event sequences")),
        "diagnostics should reject missing event sequence evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder watch mode contracts are rejected.
#[test]
fn watch_mode_hot_reload_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define watch mode later", complete_contract());

    let diagnostics = validate_watch_mode_hot_reload_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder watch mode hot-reload text")),
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
