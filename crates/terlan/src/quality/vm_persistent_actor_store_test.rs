use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_store, validate_entries_for_placeholder_terms,
    validate_no_placeholder_report_entries,
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
            "terlan-vm-persistent-actor-store-{name}-{}-{unique}",
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
        self.write(
            "crates/terlan/src/runtime/vm/process.rs",
            r#"
pub(crate) struct VmProcessId pub(crate) enum VmProcessState
pub(crate) struct VmProcessSource pub(crate) struct VmMessage
pub(crate) struct VmProcess mailbox: VecDeque<VmMessage> mailbox_len
receive_next selective_receive resource_handles exit_process
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer.rs",
            r#"
pub(crate) struct VmTimerId pub(crate) enum VmTimerKind
pub(crate) enum VmTimerEvent pub(crate) struct VmTimerSnapshot
pub(crate) struct VmTimerTable start_one_shot start_receive_timeout
cancel_owner_timers advance_clock snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
pub(crate) struct VmResourceId pub(crate) enum VmResourceTransferPolicy
pub(crate) struct VmResourceDescriptor pub(crate) struct VmResourceRecord
pub(crate) struct VmResourceSnapshot pub(crate) enum VmResourceEvent
cleanup_owner snapshots stale native resource handle
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
pub(crate) struct VmDistributedStorageSnapshot
pub(crate) enum VmDistributedStorageOutcome
pub(crate) struct VmDistributedStorageAdapter
append flush load_snapshot PartialWrite FlushTimedOut ChecksumMismatch
StaleSnapshot requires_recovery recovery_action
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state.rs",
            r#"
pub(crate) struct VmDistributedStateEntry
pub(crate) struct VmDistributedStateVersion
pub(crate) enum VmDistributedStateWriteOutcome
export_snapshot import_snapshot Replayed Conflict
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_scheduler/mod.rs",
            r#"
pub(crate) enum VmMigrationPhase pub(crate) struct VmMigrationIntent
pub(crate) enum VmMigrationOutcome VmSchedulerEventKind
MigrationPhaseAdvanced MigrationCommitted MigrationRolledBack
completed_migration_outcomes commit_migration rollback_migration
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_store.rs",
            r#"
pub(crate) struct VmPersistentActorId
pub(crate) struct VmPersistentActorSchema
pub(crate) struct VmPersistentActorDeclaration
pub(crate) struct VmPersistentActorSnapshot
pub(crate) struct VmPersistentActorEvent
pub(crate) struct VmPersistentActorReplay
pub(crate) enum VmPersistentActorStoreOutcome
pub(crate) trait VmPersistentActorStoreAdapter
pub(crate) struct VmInMemoryPersistentActorStore
pub(crate) struct VmFileBackedPersistentActorStore
pub(crate) struct VmEmbeddedKeyValuePersistentActorStore
pub(crate) struct VmDatabaseBackedPersistentActorStore
store_snapshot append_event reject_partial_event events_after
PartialWriteRejected IncompatibleSchema open_file_backed
new_embedded_key_value from_embedded_key_values export_key_values
new_database_backed from_database_rows export_database_rows
database_backed_sql_statements
persistent actor file-backed log is corrupt
persistent actor embedded key/value store is corrupt
persistent actor database-backed row is corrupt
persistent actor storage lane must be non-empty
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process_test.rs",
            r#"
process_table_sends_ordered_messages_and_wakes_recipient
process_selective_receive_preserves_skipped_messages
process_exit_clears_mailbox_and_returns_resource_handles
process_selective_receive_preserves_large_skipped_mailbox_prefix
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer_test.rs",
            r#"
timer_table_starts_one_shot_timer_and_exposes_snapshot
timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order
timer_table_receive_timeout_wakes_blocked_process
timer_table_fires_equal_deadlines_in_timer_id_order
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource_cancellation_test.rs",
            "cancelled_process_resource_cleanup_makes_handles_stale",
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_reports_finalize_and_partial_write_failures
vm_distributed_storage_reports_flush_timeout_with_retry_recovery
vm_distributed_storage_detects_corrupt_snapshot_checksum
vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark
vm_distributed_storage_recovered_snapshots_preserve_sequence_watermark
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_scheduler/distributed_scheduler_test/migration.rs",
            r#"
vm_distributed_scheduler_replays_duplicate_terminal_migration_outcomes
vm_distributed_scheduler_rolls_back_storage_timeout_idempotently
vm_distributed_scheduler_rolls_back_storage_partial_write_idempotently
vm_distributed_scheduler_replays_duplicate_partial_commit_rollbacks_without_duplicate_envelopes
"#,
        )?;
        self.write(
            "std/vm/DistributedStorageTest.terl",
            r#"
reopen_preserves_snapshots_and_sequence_watermark
closed_adapter_lifecycle_failures_are_typed
cluster_adapter_capability_is_explicit
"#,
        )?;
        self.write(
            "std/vm/PersistentActorTest.terl",
            r#"
typed_snapshot_schema_id_is_source_visible
PersistentActor.schema_id
PersistentActor.snapshot
PersistentActor.replay
PersistentActor.compatible_schema
resource_restore_plan_is_source_visible
PersistentActor.resource_checkpoint
PersistentActor.restore_resource
timer_restore_plan_is_source_visible
PersistentActor.timer_checkpoint
PersistentActor.restore_timer
mailbox_restore_plan_is_source_visible
PersistentActor.mailbox_checkpoint
PersistentActor.restore_mailbox
package_store_binding_is_source_visible
PersistentActor.package_store
PersistentActorDeclaration
persistent_actor_declaration_is_source_visible
PersistentActor.persistent_actor
	"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_store_test.rs",
            r#"
vm_persistent_actor_store_replays_snapshot_and_events_deterministically
vm_persistent_actor_store_rejects_stale_snapshot_and_schema_drift
vm_persistent_actor_store_rejects_duplicate_and_partial_events_without_mutation
vm_persistent_actor_store_restores_mailbox_timer_and_resource_checkpoints
vm_persistent_actor_store_rejects_invalid_ids_schema_versions_and_handles
vm_persistent_actor_declaration_binds_actor_schema_and_storage_lane
vm_persistent_actor_declaration_rejects_invalid_storage_lanes
vm_file_backed_persistent_actor_store_reopens_snapshot_and_events
vm_file_backed_persistent_actor_store_rejects_corrupt_log
vm_embedded_key_value_persistent_actor_store_exports_and_restores_snapshot_and_events
vm_embedded_key_value_persistent_actor_store_rejects_corrupt_records
vm_database_backed_persistent_actor_store_exports_sql_rows_and_replays
vm_database_backed_persistent_actor_store_rejects_corrupt_rows_and_table_names
"#,
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-persistent-actor-store-check: vm-model-sync-store-check
	$(MAKE) vm-process-model-check
	$(MAKE) vm-timer-primitives-check
	$(MAKE) vm-resource-ownership-check
	$(MAKE) vm-distributed-transport-check
	$(TERLC) check std/vm/PersistentActorTest.terl
	bash scripts/run_exact_cargo_test.sh -p terlan formal_pipeline::formal_pipeline_test::persistence_and_effect_interfaces::embedded_std_interfaces_include_vm_persistent_actor_contract -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_persistent_actor_store_replays_snapshot_and_events_deterministically -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_persistent_actor_declaration_binds_actor_schema_and_storage_lane -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_persistent_actor_declaration_rejects_invalid_storage_lanes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_file_backed_persistent_actor_store_reopens_snapshot_and_events -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_file_backed_persistent_actor_store_rejects_corrupt_log -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_embedded_key_value_persistent_actor_store_exports_and_restores_snapshot_and_events -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_embedded_key_value_persistent_actor_store_rejects_corrupt_records -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_database_backed_persistent_actor_store_exports_sql_rows_and_replays -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_database_backed_persistent_actor_store_rejects_corrupt_rows_and_table_names -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_store_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-store
"#;

#[test]
fn vm_persistent_actor_store_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_store(repo.root()).expect("quality check");

    assert_eq!(summary.adapter_matrix_count, 9);
    assert_eq!(summary.snapshot_event_fixture_count, 11);
    assert_eq!(summary.replay_trace_count, 6);
    assert_eq!(summary.rejected_persistent_actor_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-store-report-v1"));
    assert!(report.contains("process mailbox fixture"));
    assert!(report.contains("persistent actor store fixture"));
    assert!(report.contains("source-visible persistent actor schema fixture"));
    assert!(report.contains("source-visible resource restore API fixture"));
    assert!(report.contains("source-visible mailbox restore API fixture"));
    assert!(report.contains("source-visible timer restore API fixture"));
    assert!(report.contains("package-provided store adapter: source-visible package store binding"));
    assert!(report
        .contains("source-visible persistent actor declaration: typed actor/schema storage lane"));
    assert!(report.contains(
        "source-visible persistent actor declaration fixture: actor id, schema, storage lane"
    ));
    assert!(report
        .contains("file-backed persistent actor adapter: deterministic typed file log replay"));
    assert!(report.contains(
        "file-backed persistent actor fixture: reopen typed log and reject corrupt records"
    ));
    assert!(report.contains(
        "embedded key/value persistent actor adapter: deterministic VM-owned keyspace replay"
    ));
    assert!(report.contains(
        "embedded key/value persistent actor fixture: export typed keyspace and reject corrupt records"
    ));
    assert!(
        report.contains("database-backed persistent actor adapter: deterministic SQL row replay")
    );
    assert!(report.contains(
        "database-backed persistent actor fixture: export SQL rows and reject corrupt records"
    ));
    assert!(report.contains("partial snapshot write requires checkpoint rewrite"));
    assert!(!report.contains("file-backed storage adapter trait implementation"));
    assert!(!report.contains("embedded key/value adapter trait implementation"));
    assert!(!report.contains("database-backed storage adapter trait implementation"));
    assert!(!report.contains("public persistent actor declaration syntax"));
    assert!(!report.contains("package-provided store adapter integration"));
    assert!(!report.contains("typed actor snapshot schema id exposed to Terlan code"));
    assert!(!report.contains("durable resource handle restore API"));
    assert!(!report.contains("mailbox checkpoint restore API for actor restart"));
    assert!(!report.contains("timer restore API for actor restart"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_store_rejects_missing_mailbox_anchor() {
    let repo = TestRepo::new("missing-mailbox").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/process.rs");
    let source = fs::read_to_string(&path).expect("process source");
    repo.write(
        "crates/terlan/src/runtime/vm/process.rs",
        &source.replace("mailbox: VecDeque<VmMessage>", ""),
    )
    .expect("rewrite process source");

    let error = run_vm_persistent_actor_store(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("mailbox: VecDeque"));
}

#[test]
fn vm_persistent_actor_store_rejects_missing_storage_recovery_anchor() {
    let repo = TestRepo::new("missing-storage-recovery").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_storage.rs");
    let source = fs::read_to_string(&path).expect("storage source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &source.replace("FlushTimedOut", ""),
    )
    .expect("rewrite storage source");

    let error = run_vm_persistent_actor_store(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("FlushTimedOut"));
}

#[test]
fn vm_persistent_actor_store_rejects_missing_schema_drift_test_anchor() {
    let repo = TestRepo::new("missing-schema-drift-test").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_store_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor store test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_store_test.rs",
        &source.replace(
            "vm_persistent_actor_store_rejects_stale_snapshot_and_schema_drift",
            "",
        ),
    )
    .expect("rewrite persistent actor store test source");

    let error = run_vm_persistent_actor_store(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("vm_persistent_actor_store_rejects_stale_snapshot_and_schema_drift"));
}

#[test]
fn vm_persistent_actor_store_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) vm-resource-ownership-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_store(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-resource-ownership-check"));
}

#[test]
fn vm_persistent_actor_store_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor store report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "snapshot/event fixtures",
        &["todo persistent actor snapshot fixture"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
