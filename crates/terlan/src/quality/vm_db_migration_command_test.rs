use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::{
    parse_migration_filename, run_vm_db_migration_command, validate_report, MIGRATION_FIXTURES,
    SOURCE_CONTRACTS,
};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-db-migration-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        for (relative, anchors) in SOURCE_CONTRACTS {
            self.write(relative, &anchors.join("\n"))?;
        }
        for (filename, _) in MIGRATION_FIXTURES {
            self.write(
                &format!("crates/terlan/src/commands/db/testdata/{filename}"),
                "-- +terlan Up\nSELECT 1;\n",
            )?;
        }
        self.write(
            "Makefile",
            "vm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check\n\tcargo run -p terlan --bin terlan-quality --quiet -- vm-db-migration-command\n\ttest -s target/quality/vm-db-migration-report.json\nvm-sql-macro-validation-check: vm-db-migration-command-check\n",
        )
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn gate_writes_deterministic_redacted_migration_evidence() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let first = run_vm_db_migration_command(repo.root()).expect("first gate run");
    let first_text = fs::read_to_string(&first.report_path).expect("first report");
    let second = run_vm_db_migration_command(repo.root()).expect("second gate run");
    let second_text = fs::read_to_string(&second.report_path).expect("second report");

    assert_eq!(first.migration_count, 3);
    assert_eq!(first.diagnostic_count, 16);
    assert_eq!(first.contract_fingerprint, second.contract_fingerprint);
    assert_eq!(first_text, second_text);
    assert!(first_text.contains("terlan.vm-db-migration-command.v1"));
    assert!(first_text.contains("20260619123200"));
    assert!(first_text.contains("rolled-back"));
    assert!(first_text.contains("confirmation_flag_required"));
    assert!(first_text.contains("protected_transport_rejected"));
    assert!(first_text.contains("duplicate-id"));
    assert!(first_text.contains("migration-failed"));
    assert!(first_text.contains("dirty-schema"));
    assert!(first_text.contains("snapshot-corrupt"));
    assert!(first_text.contains("snapshot-drift"));
    assert!(first_text.contains("snapshot-unsupported-contract"));
    assert!(first_text.contains("file-missing"));
    assert!(first_text.contains("checksum-mismatch"));
    assert!(first_text.contains("name-mismatch"));
    assert!(first_text.contains("applied_at_metadata"));
    assert!(first_text.contains("rfc3339-utc-microseconds"));
    assert!(first_text.contains("\"wall_clock_values_reported\": false"));
    assert!(!first_text.contains("SELECT 1"));
}

#[test]
fn gate_rejects_missing_applied_at_utc_normalization_anchor() {
    let repo = TestRepo::new("missing-applied-at-utc").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let status_path = "crates/terlan/src/commands/db/status.rs";
    let source = fs::read_to_string(repo.root().join(status_path)).expect("status fixture");
    repo.write(status_path, &source.replace("AT TIME ZONE 'UTC'", ""))
        .expect("rewrite status fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("AT TIME ZONE 'UTC'"));
}

#[test]
fn gate_rejects_missing_destructive_confirmation_anchor() {
    let repo = TestRepo::new("missing-confirmation").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let command_path = "crates/terlan/src/commands/db/mod.rs";
    let source = fs::read_to_string(repo.root().join(command_path)).expect("command fixture");
    repo.write(command_path, &source.replace("requires --confirm", ""))
        .expect("rewrite command fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("requires --confirm"));
}

#[test]
fn gate_rejects_missing_out_of_order_history_anchor() {
    let repo = TestRepo::new("missing-out-of-order").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let status_path = "crates/terlan/src/commands/db/status.rs";
    let source = fs::read_to_string(repo.root().join(status_path)).expect("status fixture");
    repo.write(
        status_path,
        &source.replace("error[db.migration.out_of_order]", ""),
    )
    .expect("rewrite status fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[db.migration.out_of_order]"));
}

#[test]
fn gate_rejects_missing_checksum_mismatch_anchor() {
    let repo = TestRepo::new("missing-checksum-mismatch").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let status_path = "crates/terlan/src/commands/db/status.rs";
    let source = fs::read_to_string(repo.root().join(status_path)).expect("status fixture");
    repo.write(
        status_path,
        &source.replace("error[db.migration.checksum_mismatch]", ""),
    )
    .expect("rewrite status fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[db.migration.checksum_mismatch]"));
}

#[test]
fn gate_rejects_missing_duplicate_migration_id_anchor() {
    let repo = TestRepo::new("missing-duplicate-id").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let migration_path = "crates/terlan/src/commands/db/migration.rs";
    let source = fs::read_to_string(repo.root().join(migration_path)).expect("migration fixture");
    repo.write(
        migration_path,
        &source.replace("error[db.migration.duplicate_id]", ""),
    )
    .expect("rewrite migration fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[db.migration.duplicate_id]"));
}

#[test]
fn gate_rejects_missing_atomic_rollback_anchor() {
    let repo = TestRepo::new("missing-rollback").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let execution_path = "crates/terlan/src/commands/db/execution.rs";
    let source = fs::read_to_string(repo.root().join(execution_path)).expect("execution fixture");
    repo.write(
        execution_path,
        &source.replace("finish_transaction(transaction, false)", ""),
    )
    .expect("rewrite execution fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("finish_transaction(transaction, false)"));
}

#[test]
fn gate_rejects_missing_migration_failed_anchor() {
    let repo = TestRepo::new("missing-migration-failed").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let execution_path = "crates/terlan/src/commands/db/execution.rs";
    let source = fs::read_to_string(repo.root().join(execution_path)).expect("execution fixture");
    repo.write(
        execution_path,
        &source.replace("error[db.migration.failed]", ""),
    )
    .expect("rewrite execution fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[db.migration.failed]"));
}

#[test]
fn gate_rejects_missing_dirty_schema_anchor() {
    let repo = TestRepo::new("missing-dirty-schema").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let snapshot_path = "crates/terlan/src/commands/db/snapshot.rs";
    let source = fs::read_to_string(repo.root().join(snapshot_path)).expect("snapshot fixture");
    repo.write(snapshot_path, &source.replace("error[db.schema.dirty]", ""))
        .expect("rewrite snapshot fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[db.schema.dirty]"));
}

#[test]
fn gate_rejects_missing_snapshot_corruption_anchor() {
    let repo = TestRepo::new("missing-snapshot-corrupt").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let snapshot_path = "crates/terlan/src/database_schema.rs";
    let source = fs::read_to_string(repo.root().join(snapshot_path)).expect("snapshot fixture");
    repo.write(
        snapshot_path,
        &source.replace("error[db.snapshot.corrupt]", ""),
    )
    .expect("rewrite snapshot fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[db.snapshot.corrupt]"));
}

#[test]
fn gate_rejects_missing_snapshot_contract_anchor() {
    let repo = TestRepo::new("missing-snapshot-contract").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let snapshot_path = "crates/terlan/src/database_schema.rs";
    let source = fs::read_to_string(repo.root().join(snapshot_path)).expect("snapshot fixture");
    repo.write(
        snapshot_path,
        &source.replace("error[db.snapshot.unsupported_contract]", ""),
    )
    .expect("rewrite snapshot fixture");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("error[db.snapshot.unsupported_contract]"));
}

#[test]
fn gate_rejects_make_order_drift() {
    let repo = TestRepo::new("make-order").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        "vm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check\n\tcargo run -p terlan --bin terlan-quality --quiet -- vm-db-migration-command\n\ttest -s target/quality/vm-db-migration-report.json\nvm-sql-macro-validation-check:\n",
    )
    .expect("rewrite Makefile");

    let error = run_vm_db_migration_command(repo.root()).expect_err("gate must fail");

    assert!(error.contains("vm-sql-macro-validation-check: vm-db-migration-command-check"));
}

#[test]
fn migration_fixture_names_require_timestamp_and_snake_case() {
    assert_eq!(
        parse_migration_filename("20260619123000_create_users.sql").expect("valid fixture"),
        ("20260619123000".to_string(), "create_users".to_string())
    );
    assert!(parse_migration_filename("2026_create_users.sql")
        .expect_err("short id must fail")
        .contains("14-digit timestamp"));
    assert!(parse_migration_filename("20260619123000_CreateUsers.sql")
        .expect_err("mixed case must fail")
        .contains("lowercase snake case"));
}

#[test]
fn report_validation_rejects_credentials_and_sql_text() {
    let error = validate_report(&json!({
        "database": "postgres://alice:secret@example.test/app",
        "statement": "CREATE TABLE leaked(id int)",
    }))
    .expect_err("sensitive report must fail");

    assert!(error.contains("postgres://"));
    assert!(error.contains("CREATE TABLE"));
}
