use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_compaction, validate_entries_for_placeholder_terms,
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
            "terlan-vm-persistent-actor-compaction-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/persistent_actor_compaction.rs",
            r#"
pub(crate) struct VmPersistentActorRetentionPolicy
pub(crate) struct VmPersistentActorCompactionCandidate
pub(crate) enum VmPersistentActorCompactionError
pub(crate) fn plan_persistent_actor_compaction
RetentionBeforeSchemaMigrationFloor
RetentionBeforeAuditFloor
CompactedSnapshotNotEquivalent
RetainedEventGap
ResourceHandlePrunedWithoutPolicy
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_compaction_test.rs",
            r#"
vm_persistent_actor_compaction_accepts_equivalent_snapshot_and_suffix
vm_persistent_actor_compaction_rejects_schema_and_audit_floor_loss
vm_persistent_actor_compaction_rejects_unsafe_checkpoint_and_resource_pruning
vm_persistent_actor_compaction_rejects_bad_retained_event_suffix
vm_persistent_actor_compaction_rejects_non_equivalent_snapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
pub(crate) enum VmDistributedStorageOutcome Compacted SnapshotMissing
retained_snapshots pub(crate) fn compact retain_from_sequence
retain(|snapshot| pub(crate) fn latest_sequence StorageUnavailable
requires_recovery
VmDistributedStorageTransactionalRollbackProof
last_batch_rollback_proof
transactional_rollback_proof
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
pub(crate) struct VmResourceSnapshot pub(crate) fn cleanup_owner
VmResourceEvent::CleanedUpOnExit pub(crate) fn snapshots transfer_policy
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process.rs",
            r#"
mailbox: VecDeque<VmMessage> resource_handles: Vec<String>
pub(crate) fn exit self.mailbox.clear self.resource_handles.drain exit_process
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_compacts_old_snapshots_deterministically
vm_distributed_storage_compaction_physically_removes_pruned_snapshots_and_retains_boundary
vm_distributed_storage_compaction_preserves_monotonic_sequence_watermark
vm_distributed_storage_durable_transactional_batch_rollback_preserves_commit_boundary
VmDistributedStorageOutcome::Compacted { retained: 2 }
VmDistributedStorageOutcome::Compacted { retained: 0 }
VmDistributedStorageOutcome::SnapshotMissing
adapter.latest_sequence()
vm_distributed_storage_returns_unavailable_for_missing_backend_without_panics
VmDistributedStorageOperation::Compact
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process_test.rs",
            r#"
process_exit_clears_mailbox_and_returns_resource_handles
process_resource_removal_cancellation_and_reduction_accounting_are_stable
process.mailbox_len()
process.resource_handles.is_empty()
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource_cancellation_test.rs",
            "cancelled_process_resource_cleanup_makes_handles_stale",
        )?;
        self.write(
            "std/vm/DistributedStorageTest.terl",
            r#"
compaction_and_checksum_metadata_are_typed
adapter.compact(2)
DistributedStorage.retained_snapshots(compacted)
assert_equal("compacted", DistributedStorage.kind(compacted))
assert_equal(1, retained_snapshots)
"#,
        )?;
        self.write(
            "std/vm/PersistentActorTest.terl",
            r#"
retention_policy_is_source_visible
PersistentActor.retention_policy(
RetentionPolicy
actor_family_retention_defaults_are_source_visible
PersistentActor.family_retention_defaults(
ActorFamilyRetentionDefaults
audit_retention_plan_is_source_visible
PersistentActor.audit_retention(
AuditRetentionPlan
package_retention_policy_binding_is_source_visible
PersistentActor.package_retention_policy(
PackageRetentionPolicyBinding
model_sync_retention_continuity_is_source_visible
PersistentActor.model_sync_retention_continuity(
ModelSyncRetentionContinuityPlan
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
vm-persistent-actor-compaction-check: vm-persistent-actor-schema-check
	$(MAKE) vm-distributed-state-check
	$(MAKE) vm-resource-ownership-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_compaction_physically_removes_pruned_snapshots_and_retains_boundary -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_durable_transactional_batch_rollback_preserves_commit_boundary -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_accepts_equivalent_snapshot_and_suffix -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_schema_and_audit_floor_loss -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_unsafe_checkpoint_and_resource_pruning -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_bad_retained_event_suffix -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_non_equivalent_snapshot -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_compaction_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-compaction
"#;

#[test]
fn vm_persistent_actor_compaction_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_compaction(repo.root()).expect("quality check");

    assert_eq!(summary.before_after_store_size_count, 6);
    assert_eq!(summary.replay_equivalence_trace_count, 6);
    assert_eq!(summary.retained_range_count, 12);
    assert_eq!(summary.rejected_retention_policy_count, 6);
    assert_eq!(summary.crash_injection_case_count, 7);
    assert_eq!(summary.resource_cleanup_decision_count, 6);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-compaction-report-v1"));
    assert!(report.contains("retain_from_sequence=3 keeps checkpoints 3..latest"));
    assert!(report
        .contains("source-visible retention policy declares retain-from schema and audit floors"));
    assert!(report.contains(
        "source-visible actor-family retention defaults declare local and production policies"
    ));
    assert!(report.contains("source-visible audit retention plan declares required event evidence"));
    assert!(report
        .contains("source-visible package retention policy binding declares package ownership"));
    assert!(report.contains("source-visible model-sync retention continuity declares stream floor"));
    assert!(report
        .contains("adapter physical compaction removes pruned snapshots and retains boundary"));
    assert!(report.contains("retention before schema migration floor"));
    assert!(!report.contains("public actor retention policy syntax"));
    assert!(report.contains("process exit returns owned resource handles in stable order"));
    assert!(report.contains("durable transactional batch rollback preserves pre-commit boundary"));
    assert!(report.contains("persistent actor resource handle pruning requires explicit policy"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_compaction_rejects_missing_compact_anchor() {
    let repo = TestRepo::new("missing-compact").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_storage.rs");
    let source = fs::read_to_string(&path).expect("storage source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &source.replace("retain(|snapshot|", ""),
    )
    .expect("rewrite storage source");

    let error = run_vm_persistent_actor_compaction(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("retain(|snapshot|"));
}

#[test]
fn vm_persistent_actor_compaction_rejects_missing_resource_cleanup_anchor() {
    let repo = TestRepo::new("missing-resource-cleanup").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/resource.rs");
    let source = fs::read_to_string(&path).expect("resource source");
    repo.write(
        "crates/terlan/src/runtime/vm/resource.rs",
        &source.replace("VmResourceEvent::CleanedUpOnExit", ""),
    )
    .expect("rewrite resource source");

    let error = run_vm_persistent_actor_compaction(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmResourceEvent::CleanedUpOnExit"));
}

#[test]
fn vm_persistent_actor_compaction_rejects_missing_retained_event_gap_anchor() {
    let repo = TestRepo::new("missing-retained-event-gap").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_compaction.rs");
    let source = fs::read_to_string(&path).expect("persistent actor compaction source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_compaction.rs",
        &source.replace("RetainedEventGap", ""),
    )
    .expect("rewrite persistent actor compaction source");

    let error = run_vm_persistent_actor_compaction(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("RetainedEventGap"));
}

#[test]
fn vm_persistent_actor_compaction_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) vm-resource-ownership-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_compaction(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-resource-ownership-check"));
}

#[test]
fn vm_persistent_actor_compaction_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor compaction report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected =
        validate_entries_for_placeholder_terms("retained ranges", &["tbd retained range"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
