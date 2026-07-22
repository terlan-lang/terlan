use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_schema, validate_entries_for_placeholder_terms,
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
            "terlan-vm-persistent-actor-schema-{name}-{}-{unique}",
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
            "std/vm/PersistentActorTest.terl",
            r#"
schema_declaration_is_source_visible
PersistentActor.schema(
SchemaDeclaration
package_event_variant_schema_id_is_source_visible
PersistentActor.event_variant_schema(
EventVariantSchemaId
durable_adapter_schema_metadata_is_source_visible
PersistentActor.durable_adapter_schema(
DurableAdapterSchemaMetadata
migration_rollback_after_failed_schema_migration_is_source_visible
PersistentActor.migration_rollback(
MigrationRollbackPlan
package_migration_registration_is_source_visible
PersistentActor.register_package_migration(
PackageMigrationRegistration
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_schema.rs",
            r#"
pub(crate) struct VmPersistentActorSchemaKey
pub(crate) struct VmPersistentActorSchemaDescriptor
pub(crate) struct VmPersistentActorMigrationEdge
pub(crate) enum VmPersistentActorMigrationGuard
pub(crate) enum VmPersistentActorMigrationEffect
pub(crate) enum VmPersistentActorSchemaError
pub(crate) struct VmPersistentActorMigrationGraph
validate_event_migration_sequence DuplicateSchemaId MissingMigrationEdge
MigrationGraphCycle AmbiguousMigrationEdge NondeterministicMigrationGuard
SideEffectfulMigration WallClockDependentMigration RequiredFieldLost
UnknownEventConstructorVariant IncompatibleMailboxPayloadSchema
StalePackageSchemaVersion OutOfOrderEventMigration
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_schema_test.rs",
            r#"
vm_persistent_actor_schema_plans_deterministic_migration_chain
vm_persistent_actor_schema_rejects_duplicate_missing_and_cyclic_migrations
vm_persistent_actor_schema_rejects_unsafe_migration_guards_and_effects
vm_persistent_actor_schema_rejects_lossy_event_mailbox_and_package_changes
vm_persistent_actor_schema_rejects_out_of_order_event_migration_sequences
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state.rs",
            r#"
pub(crate) struct VmDistributedStateVersion
pub(crate) struct VmDistributedStateEntry
pub(crate) struct VmDistributedStateConflict
pub(crate) enum VmDistributedStateWriteOutcome
sequence: u64 node_id: String export_snapshot import_snapshot
snapshot version must be valid snapshot contains duplicate state scope
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
pub(crate) struct VmDistributedStorageSnapshot checkpoint_id: String
sequence: u64 checksum: u32 expected_checksum with_checksum
StaleSnapshot ChecksumMismatch requires_recovery recovery_action
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_scheduler/mod.rs",
            r#"
pub(crate) enum VmMigrationPhase pub(crate) struct VmMigrationIntent
pub(crate) enum VmMigrationOutcome completed_migration_outcomes
MigrationRolledBack already completed with incompatible outcome
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_scheduler/fault.rs",
            r#"
timeout_migration_at_tick partial_commit_migration_at_tick
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer.rs",
            r#"
pub(crate) struct VmTimerSnapshot owner: VmProcessId
deadline_tick: u64 kind: VmTimerKind snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
pub(crate) struct VmResourceSnapshot owner: VmProcessId kind: String
label: String transfer_policy: VmResourceTransferPolicy
stale native resource handle
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state_test.rs",
            r#"
vm_distributed_state_reports_conflicts_with_versions_and_policy
vm_distributed_state_exports_and_imports_deterministic_snapshots
vm_distributed_state_rejects_invalid_scopes_versions_and_snapshots
snapshot contains duplicate state scope
state version sequence must be non-zero state version node id must be non-empty
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_rejects_stale_snapshot_replay
vm_distributed_storage_detects_corrupt_snapshot_checksum
vm_distributed_storage_recovered_snapshots_preserve_sequence_watermark
vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark
vm_distributed_storage_rejects_invalid_policy_and_snapshot_descriptors
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_scheduler/distributed_scheduler_test/migration.rs",
            r#"
vm_distributed_scheduler_replays_duplicate_timeout_rollbacks_without_duplicate_envelopes
vm_distributed_scheduler_rolls_back_storage_timeout_idempotently
vm_distributed_scheduler_rolls_back_storage_partial_write_idempotently
vm_distributed_scheduler_rejects_invalid_partial_commit_inputs
vm_distributed_scheduler_rejects_invalid_migration_timeout_inputs
"#,
        )?;
        self.write(
            "std/vm/DistributedStateTest.terl",
            r#"
write_outcomes_are_explicit_and_versioned
conflict_and_policy_mismatch_are_typed
restore(snapshot)
"#,
        )?;
        self.write(
            "std/vm/DistributedStorageTest.terl",
            r#"
reopen_preserves_snapshots_and_sequence_watermark
compaction_and_checksum_metadata_are_typed
assert_equal("stale_snapshot", stale_kind)
assert_equal("reject_replay", DistributedStorage.recovery_action(stale))
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
vm-persistent-actor-schema-check: vm-persistent-actor-store-check
	$(MAKE) vm-distributed-state-check
	$(MAKE) vm-distributed-transport-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_plans_deterministic_migration_chain -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_duplicate_missing_and_cyclic_migrations -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_unsafe_migration_guards_and_effects -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_lossy_event_mailbox_and_package_changes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_out_of_order_event_migration_sequences -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_schema_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-schema
"#;

#[test]
fn vm_persistent_actor_schema_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_schema(repo.root()).expect("quality check");

    assert_eq!(summary.schema_id_count, 11);
    assert_eq!(summary.migration_graph_case_count, 8);
    assert_eq!(summary.compatibility_matrix_count, 7);
    assert_eq!(summary.rejected_migration_case_count, 13);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-schema-report-v1"));
    assert!(report.contains("persistent actor schema key"));
    assert!(report.contains("source-visible persistent actor schema declaration descriptor"));
    assert!(report.contains("source-visible package event variant schema id descriptor"));
    assert!(report.contains("source-visible durable adapter schema metadata descriptor"));
    assert!(report.contains("source-visible failed migration rollback plan descriptor"));
    assert!(report.contains("source-visible package migration registration descriptor"));
    assert!(report.contains("distributed state version"));
    assert!(report.contains("unknown actor state schema id"));
    assert!(report.contains("required field lost without default, rename, or tombstone"));
    assert!(report.contains("duplicate timeout rollback keeps event and envelope counts stable"));
    assert!(report
        .contains("source-visible persistent actor migration rollback plan can be typechecked"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_schema_rejects_missing_state_version_anchor() {
    let repo = TestRepo::new("missing-state-version").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_state.rs");
    let source = fs::read_to_string(&path).expect("state source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_state.rs",
        &source.replace("pub(crate) struct VmDistributedStateVersion", ""),
    )
    .expect("rewrite state source");

    let error = run_vm_persistent_actor_schema(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmDistributedStateVersion"));
}

#[test]
fn vm_persistent_actor_schema_rejects_missing_migration_replay_anchor() {
    let repo = TestRepo::new("missing-migration-replay").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_scheduler/mod.rs");
    let source = fs::read_to_string(&path).expect("scheduler source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_scheduler/mod.rs",
        &source.replace("already completed with incompatible outcome", ""),
    )
    .expect("rewrite scheduler source");

    let error = run_vm_persistent_actor_schema(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("already completed with incompatible outcome"));
}

#[test]
fn vm_persistent_actor_schema_rejects_missing_wall_clock_migration_anchor() {
    let repo = TestRepo::new("missing-wall-clock-migration").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_schema.rs");
    let source = fs::read_to_string(&path).expect("persistent actor schema source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_schema.rs",
        &source.replace("WallClockDependentMigration", ""),
    )
    .expect("rewrite persistent actor schema source");

    let error = run_vm_persistent_actor_schema(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("WallClockDependentMigration"));
}

#[test]
fn vm_persistent_actor_schema_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) vm-distributed-state-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_schema(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-distributed-state-check"));
}

#[test]
fn vm_persistent_actor_schema_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor schema report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "migration graph cases",
        &["todo migration graph case"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
