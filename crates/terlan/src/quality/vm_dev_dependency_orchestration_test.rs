use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_dev_dependency_orchestration, DB_DEPENDENCY_PREPARE_CALL, SOURCE_CONTRACTS};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-dev-dependency-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create fixture");
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, text).expect("write fixture");
    }

    fn write_complete_fixture(&self) {
        for (relative, anchors) in SOURCE_CONTRACTS {
            let mut source = anchors.join("\n");
            if *relative == "crates/terlan/src/commands/db/mod.rs" {
                source.push('\n');
                source.push_str(&[DB_DEPENDENCY_PREPARE_CALL; 4].join("\n"));
            }
            self.write(relative, &source);
        }
        self.write(
            "Makefile",
            "vm-dev-dependency-orchestration-check:\n\tcargo test -p terlan --lib commands::dev_dependencies\n\tcargo test -p terlan --lib --features quality-tools vm_dev_dependency_orchestration\n\tcargo run -p terlan --bin terlan-quality --features quality-tools --quiet -- vm-dev-dependency-orchestration\n\ttest -s target/quality/vm-dev-dependency-report.json\nvm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check\n",
        );
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn gate_writes_deterministic_shared_dependency_evidence() {
    let repo = TestRepo::new("complete");
    repo.write_complete_fixture();

    let first = run_vm_dev_dependency_orchestration(repo.root()).expect("first gate");
    let first_text = fs::read_to_string(&first.report_path).expect("first report");
    let second = run_vm_dev_dependency_orchestration(repo.root()).expect("second gate");
    let second_text = fs::read_to_string(&second.report_path).expect("second report");

    assert_eq!(first.command_count, 8);
    assert_eq!(first.diagnostic_count, 5);
    assert_eq!(first.contract_fingerprint, second.contract_fingerprint);
    assert_eq!(first_text, second_text);
    assert!(first_text.contains("terlan.vm-dev-dependency-orchestration.v1"));
    assert!(first_text.contains("db rebuild --dev --confirm"));
    assert!(first_text.contains("never-start-local-dependencies"));
    assert!(first_text.contains("collect-logs"));
    assert!(first_text.contains("redact-and-bound-log-excerpts"));
    assert!(first_text.contains("preserve-external"));
    assert!(first_text.contains("stop-and-remove-owned"));
    assert!(first_text.contains("reuse-stopped-container"));
    assert!(first_text.contains("preserve-stale-volumes"));
    assert!(first_text.contains("sql-validation-snapshot-discovery"));
    assert!(first_text.contains("\"remaining_lifecycle\": []"));
    assert!(!first_text.contains("POSTGRES_PASSWORD"));
}

#[test]
fn gate_rejects_missing_db_dependency_wiring() {
    let repo = TestRepo::new("missing-db-wiring");
    repo.write_complete_fixture();
    let path = "crates/terlan/src/commands/db/mod.rs";
    let source = fs::read_to_string(repo.root().join(path)).expect("read fixture");
    repo.write(
        path,
        &source.replace("prepare_local_database_dependencies", ""),
    );

    let error = run_vm_dev_dependency_orchestration(repo.root()).expect_err("gate must fail");

    assert!(error.contains("prepare_local_database_dependencies"));
}

#[test]
fn gate_rejects_missing_typed_docker_diagnostic() {
    let repo = TestRepo::new("missing-docker-diagnostic");
    repo.write_complete_fixture();
    let path = "crates/terlan/src/commands/dev_dependencies.rs";
    let source = fs::read_to_string(repo.root().join(path)).expect("read fixture");
    repo.write(
        path,
        &source.replace("error[dev_dependency.docker_missing]", ""),
    );

    let error = run_vm_dev_dependency_orchestration(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[dev_dependency.docker_missing]"));
}

#[test]
fn gate_rejects_missing_log_redaction() {
    let repo = TestRepo::new("missing-log-redaction");
    repo.write_complete_fixture();
    let path = "crates/terlan/src/commands/dev_dependencies.rs";
    let source = fs::read_to_string(repo.root().join(path)).expect("read fixture");
    repo.write(path, &source.replace("redact_compose_logs", ""));

    let error = run_vm_dev_dependency_orchestration(repo.root()).expect_err("gate must fail");

    assert!(error.contains("redact_compose_logs"));
}

#[test]
fn gate_rejects_missing_external_preservation_proof() {
    let repo = TestRepo::new("missing-external-preservation");
    repo.write_complete_fixture();
    let path = "crates/terlan/src/commands/dev_dependencies.rs";
    let source = fs::read_to_string(repo.root().join(path)).expect("read fixture");
    repo.write(path, &source.replace("DependencyOwnership::External", ""));

    let error = run_vm_dev_dependency_orchestration(repo.root()).expect_err("gate must fail");

    assert!(error.contains("DependencyOwnership::External"));
}

#[test]
fn gate_rejects_db_ordering_drift() {
    let repo = TestRepo::new("ordering-drift");
    repo.write_complete_fixture();
    repo.write(
        "Makefile",
        "vm-dev-dependency-orchestration-check:\n\tcargo test -p terlan --lib --features quality-tools vm_dev_dependency_orchestration\n\tcargo run -p terlan --bin terlan-quality --features quality-tools --quiet -- vm-dev-dependency-orchestration\n\ttest -s target/quality/vm-dev-dependency-report.json\nvm-db-migration-command-check: db-command-check\n",
    );

    let error = run_vm_dev_dependency_orchestration(repo.root()).expect_err("gate must fail");

    assert!(error.contains("vm-db-migration-command-check"));
}
