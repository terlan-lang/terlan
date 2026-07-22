use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the release failure reproduction gate writes the roadmap-required
/// report.
#[test]
fn release_failure_reproduction_writes_report() {
    let repo = TempRepo::new("release_failure_reproduction_writes_report");
    repo.write(
        "docs/release/RELEASE_FAILURE_REPRODUCTION.md",
        &complete_contract(),
    );

    let summary =
        run_release_failure_reproduction(repo.path()).expect("release failure reproduction gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        ADVERSARIAL_REPRODUCTION_FIXTURES.len(),
        summary.adversarial_case_count
    );
    assert_eq!(
        "target/quality/release-failure-reproduction-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/release-failure-reproduction-report.json"),
    )
    .expect("read release failure reproduction report");
    assert!(report.contains("terlan.release-failure-reproduction.v1"));
    assert!(report.contains("release failure reproduction contract"));
    assert!(report.contains("reproduction_commands"));
    assert!(report.contains("adversarial_reproduction_fixtures"));
}

/// Verifies absolute-checkout dependency claims are rejected.
#[test]
fn release_failure_reproduction_rejects_absolute_checkout_claims() {
    let text = format!(
        "{}\nreproduction may depend on absolute checkout paths",
        complete_contract()
    );

    let diagnostics = validate_release_failure_reproduction_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("absolute checkout paths")),
        "diagnostics should reject absolute checkout claims: {diagnostics:?}"
    );
}

/// Verifies random seed evidence is required.
#[test]
fn release_failure_reproduction_rejects_missing_random_seed() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "random seed")
        .filter(|term| *term != "missing seeds")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_release_failure_reproduction_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("random seed")),
        "diagnostics should reject missing random seed evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder reproduction text is rejected.
#[test]
fn release_failure_reproduction_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define reproduction later", complete_contract());

    let diagnostics = validate_release_failure_reproduction_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder release failure reproduction text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

/// Verifies a complete reproduction sample is accepted.
#[test]
fn release_failure_reproduction_accepts_complete_reproduction_sample() {
    let diagnostics = validate_reproduction_sample(complete_reproduction_sample());

    assert!(
        diagnostics.is_empty(),
        "complete reproduction sample should pass: {diagnostics:?}"
    );
}

/// Verifies reproduction samples require seed/profile/cache/shard env vars.
#[test]
fn release_failure_reproduction_rejects_sample_missing_seed_env() {
    let diagnostics = validate_reproduction_sample(
        r#"{
            "exact_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "required_environment_variables": {
                "TERLAN_TARGET_PROFILE": "vm",
                "TERLAN_CACHE_MODE": "cold",
                "TERLAN_SHARD_ID": "0"
            },
            "input_fixture_path": "fixtures/release/failure.json",
            "random_seed": "123",
            "target_profile": "vm",
            "cache_mode": "cold",
            "shard_id": "0",
            "report_path": "target/quality/release-failure-reproduction-report.json",
            "support_bundle_path": "target/support/release-failure.json",
            "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "broader_reproduction_command": "make release-failure-reproduction-check",
            "command_success_status": "passes"
        }"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("TERLAN_REPRO_SEED")),
        "diagnostics should reject missing seed env var: {diagnostics:?}"
    );
}

/// Verifies fixture paths cannot depend on checkout-local absolute paths.
#[test]
fn release_failure_reproduction_rejects_absolute_fixture_paths() {
    let diagnostics = validate_reproduction_sample(&complete_reproduction_sample().replace(
        "fixtures/release/failure.json",
        "/home/anatoly/terlan/failure.json",
    ));

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("absolute checkout paths")),
        "diagnostics should reject absolute fixture paths: {diagnostics:?}"
    );
}

/// Verifies reproduction commands cannot depend on cargo build cache paths.
#[test]
fn release_failure_reproduction_rejects_stale_target_cache_commands() {
    let diagnostics = validate_reproduction_sample(
        &complete_reproduction_sample().replace(
            "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
            "target/debug/deps/terlan_quality-abc123 --exact release_failure_reproduction_test",
        ),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("stale caches")),
        "diagnostics should reject stale cache commands: {diagnostics:?}"
    );
}

/// Verifies CI-only environment assumptions are rejected.
#[test]
fn release_failure_reproduction_rejects_ci_only_state() {
    let diagnostics = validate_reproduction_sample(&complete_reproduction_sample().replace(
        r#""TERLAN_SHARD_ID": "0""#,
        r#""TERLAN_SHARD_ID": "0", "GITHUB_ACTIONS": "true""#,
    ));

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("CI-only state")),
        "diagnostics should reject CI-only state: {diagnostics:?}"
    );
}

/// Verifies hidden shell expansions are rejected in reproduction metadata.
#[test]
fn release_failure_reproduction_rejects_hidden_environment_values() {
    let diagnostics = validate_reproduction_sample(
        &complete_reproduction_sample().replace("\"123\"", "\"${SEED}\""),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("hidden environment assumptions")),
        "diagnostics should reject hidden environment values: {diagnostics:?}"
    );
}

fn complete_reproduction_sample() -> &'static str {
    r#"{
        "exact_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
        "required_environment_variables": {
            "TERLAN_REPRO_SEED": "123",
            "TERLAN_TARGET_PROFILE": "vm",
            "TERLAN_CACHE_MODE": "cold",
            "TERLAN_SHARD_ID": "0"
        },
        "input_fixture_path": "fixtures/release/failure.json",
        "random_seed": "123",
        "target_profile": "vm",
        "cache_mode": "cold",
        "shard_id": "0",
        "report_path": "target/quality/release-failure-reproduction-report.json",
        "support_bundle_path": "target/support/release-failure.json",
        "narrow_reproduction_command": "bash scripts/run_exact_cargo_test.sh -p terlan release_failure_reproduction_test -- --exact",
        "broader_reproduction_command": "make release-failure-reproduction-check",
        "command_success_status": "passes"
    }"#
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
