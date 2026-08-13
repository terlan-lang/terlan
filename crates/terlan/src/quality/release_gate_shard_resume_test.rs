use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

fn complete_makefile() -> String {
    let mut text = String::from(concat!(
        "CHECK_GATES := \\\n",
        "\tcompiler-check\n\n",
        "check: rust-test-suite terlan-self-validation-bootstrap\n",
        "\tTERLAN_RUST_SUITE_ALREADY_RUN=1 \\\n",
        "\tTERLAN_VALIDATION_BOOTSTRAPPED=1 \\\n",
        "\t\t$(MAKE) --no-print-directory check-gates\n\n",
        "check-gates: $(CHECK_GATES)\n\n",
        "release-0-0-7-evidence-refresh: check release-failure-reproduction-check\n",
        "\t@echo refreshed\n\n",
        "release-0-0-7-preflight:\n",
        "\ttest -s release-evidence.json\n",
        "\tterlan-vm run release-preflight.tvm\n\n",
        "lean-proof-track-release-closeout-check: rust-test-suite\n",
        "\tcargo run --bin terlan-lean-proof-closeout\n\n",
    ));
    for &(target, prerequisite) in RELEASE_GATE_CHAIN {
        text.push_str(&format!(
            "{target}: {prerequisite}\n\tterlan-quality -- {target}\n\n"
        ));
    }
    text
}

/// Verifies final composition cannot acquire an expensive prerequisite graph.
#[test]
fn release_gate_shard_resume_rejects_preflight_replay() {
    let makefile = complete_makefile().replace(
        "release-0-0-7-preflight:\n",
        "release-0-0-7-preflight: check\n\t$(MAKE) check-gates\n",
    );

    let diagnostics = validate_release_makefile(&makefile);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .contains("must compose existing evidence without prerequisite gates")),
        "diagnostics should reject preflight prerequisites: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must not execute `$(MAKE)`")),
        "diagnostics should reject preflight sub-makes: {diagnostics:?}"
    );
}

/// Verifies release-only gates cannot leak back into ordinary validation.
#[test]
fn release_gate_shard_resume_rejects_release_chain_in_check() {
    let makefile = complete_makefile().replace(
        "CHECK_GATES := \\\n\tcompiler-check",
        "CHECK_GATES := \\\n\tcompiler-check \\\n\trelease-failure-reproduction-check",
    );

    let diagnostics = validate_release_makefile(&makefile);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("ordinary `CHECK_GATES` must not own release-only")),
        "diagnostics should reject release-only work in ordinary checks: {diagnostics:?}"
    );
}

/// Verifies the release closeout cannot reintroduce a second full Rust suite.
#[test]
fn release_gate_shard_resume_rejects_duplicate_closeout_suite() {
    let makefile = complete_makefile().replace(
        "\tcargo run --bin terlan-lean-proof-closeout\n",
        "\t$(RUST_TEST) --locked -p terlan --lib --features quality-tools\n\tcargo run --bin terlan-lean-proof-closeout\n",
    );

    let diagnostics = validate_release_makefile(&makefile);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must not rerun the complete Rust library suite")),
        "diagnostics should reject duplicate suite ownership: {diagnostics:?}"
    );
}

/// Verifies the release gate shard/resume gate writes the roadmap-required
/// report.
#[test]
fn release_gate_shard_resume_writes_report() {
    let repo = TempRepo::new("release_gate_shard_resume_writes_report");
    repo.write(
        "docs/release/RELEASE_GATE_SHARD_RESUME.md",
        &complete_contract(),
    );
    repo.write("Makefile", &complete_makefile());

    let summary =
        run_release_gate_shard_resume(repo.path()).expect("release gate shard/resume gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/release-gate-shard-resume-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/release-gate-shard-resume-report.json"),
    )
    .expect("read release gate shard/resume report");
    assert!(report.contains("terlan.release-gate-shard-resume.v1"));
    assert!(report.contains("release gate shard/resume contract"));
    assert!(report.contains("gate_dag"));
}

/// Verifies completed release gates cannot regress to recursive sub-makes.
#[test]
fn release_gate_shard_resume_rejects_recursive_completed_gate() {
    let makefile = complete_makefile().replace(
        "package-registry-publish-check: package-resolver-reproducibility-check\n",
        "package-registry-publish-check:\n\t$(MAKE) package-resolver-reproducibility-check\n",
    );

    let diagnostics = validate_release_makefile(&makefile);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.contains(
            "`package-registry-publish-check` must declare `package-resolver-reproducibility-check`"
        )
        }),
        "diagnostics should reject a recursive completed gate: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("must not recursively invoke completed release gates")),
        "diagnostics should reject recursive sub-makes: {diagnostics:?}"
    );
}

/// Verifies repeated rerun claims are rejected.
#[test]
fn release_gate_shard_resume_rejects_redundant_rerun_claims() {
    let text = format!(
        "{}\nrepeated release invocations re-run completed gates without an input change",
        complete_contract()
    );

    let diagnostics = validate_release_gate_shard_resume_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("re-run completed gates")),
        "diagnostics should reject redundant rerun claims: {diagnostics:?}"
    );
}

/// Verifies gate DAG report evidence is required.
#[test]
fn release_gate_shard_resume_rejects_missing_gate_dag() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "gate DAG")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_release_gate_shard_resume_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("gate DAG")),
        "diagnostics should reject missing gate DAG evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder shard/resume text is rejected.
#[test]
fn release_gate_shard_resume_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define shard resume later", complete_contract());

    let diagnostics = validate_release_gate_shard_resume_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder release gate shard/resume text")),
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
