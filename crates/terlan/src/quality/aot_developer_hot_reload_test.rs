use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_repo() -> TempRepo {
    let repo = TempRepo::new("aot_developer_hot_reload");
    repo.write(
        "docs/compiler/AOT_DEVELOPER_HOT_RELOAD.md",
        &DOC_TERMS.join("\n"),
    );
    repo.write(
        "crates/terlan/src/commands/serve/handler_cache/source_generation.rs",
        &IMPLEMENTATION_TERMS.join("\n"),
    );
    repo.write(
        "crates/terlan/src/commands/serve/handler_cache_generation_test.rs",
        &TEST_TERMS.join("\n"),
    );
    repo
}

#[test]
fn aot_developer_hot_reload_writes_release_evidence() {
    let repo = complete_repo();
    let summary = run_aot_developer_hot_reload(repo.path()).expect("reload quality gate");
    assert_eq!(TEST_TERMS.len(), summary.adversarial_case_count);
    let report = fs::read_to_string(repo.path().join(summary.report_path)).expect("read report");
    assert!(report.contains("atomic-active-generation-pointer"));
    assert!(report.contains("\"interpreter\": false"));
}

#[test]
fn aot_developer_hot_reload_rejects_missing_atomic_publication() {
    let repo = complete_repo();
    repo.write(
        "docs/compiler/AOT_DEVELOPER_HOT_RELOAD.md",
        &DOC_TERMS
            .iter()
            .copied()
            .filter(|term| *term != "single synced `active.json`")
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let error = run_aot_developer_hot_reload(repo.path()).expect_err("missing atomic contract");
    assert!(error.contains("active.json"), "{error}");
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

    fn write(&self, relative: &str, text: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, text).expect("write fixture");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
