use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_restore, validate_entries_for_placeholder_terms,
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
            "terlan-vm-persistent-actor-restore-{name}-{}-{unique}",
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
            "crates/terlan/src/vm/main.rs",
            r#"
ExportPersistentActor
RestorePersistentActor
parse_export_persistent_actor_args
parse_restore_persistent_actor_args
render_persistent_actor_export_manifest
render_persistent_actor_restore_plan
export-persistent-actor
restore-persistent-actor
"#,
        )?;
        self.write(
            "crates/terlan/src/vm/main_test.rs",
            r#"
parse_persistent_actor_export_command_accepts_manifest_metadata
persistent_actor_export_command_renders_portable_manifest_without_payloads
parse_persistent_actor_restore_command_accepts_validation_metadata
persistent_actor_restore_command_renders_validated_plan_without_payloads
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_restore.rs",
            r#"
VmPersistentActorExport VmPersistentActorRestoreTarget
VmPersistentActorRestoreCapabilities plan_persistent_actor_restore
CorruptExportChecksum WrongActorOwner MissingDurableResourceHandle
StaleSchema ReorderedRetainedEventSuffix IncompatibleAdapterForCompactedSnapshot
VmPersistentActorCompactionRestore compaction_restore compacted_through_sequence
VmPersistentActorReplayFixture VmPersistentActorRestoreExecution
VmPersistentActorCrossMachineExport
execute_persistent_actor_restore build_cross_machine_actor_export
generate_minimal_actor_replay_fixture render_manifest
ReorderedMailboxCheckpoint validate_mailbox_checkpoint_order
IncompatibleAdapterKind source_adapter_kind allow_cross_adapter_restore StoreRejected
InvalidCrossMachineExportSource
VmPersistentActorModelSyncContinuity MissingModelSyncContinuity
ReorderedModelSyncStream model_sync_streams
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_restore_test.rs",
            r#"
vm_persistent_actor_restore_accepts_deterministic_export_plan
vm_persistent_actor_restore_rejects_corrupt_export_and_stale_schema
vm_persistent_actor_restore_rejects_wrong_actor_and_missing_resource
vm_persistent_actor_restore_rejects_reordered_event_suffix
vm_persistent_actor_restore_rejects_reordered_mailbox_checkpoint
vm_persistent_actor_restore_gates_compacted_snapshot_and_resource_adapter_support
vm_persistent_actor_restore_rejects_incompatible_adapter_kind
vm_persistent_actor_restore_executes_cross_adapter_restore
vm_persistent_actor_restore_builds_cross_machine_export_format
vm_persistent_actor_restore_accepts_compacted_export_with_restore_boundary
vm_persistent_actor_restore_validates_model_sync_stream_continuity
vm_persistent_actor_restore_rejects_missing_and_reordered_model_sync_stream
vm_persistent_actor_restore_generates_minimal_replay_fixture_without_payloads
pending_message
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state.rs",
            r#"
export_snapshot import_snapshot snapshot contains duplicate state scope
snapshot version must be valid BTreeMap<VmDistributedStateScope
VmDistributedStateVersion
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
VmDistributedStorageSnapshot load_snapshot SnapshotLoaded SnapshotMissing
ChecksumMismatch expected_checksum checkpoint_id sequence
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer.rs",
            r#"
pub(crate) struct VmTimerSnapshot owner: VmProcessId deadline_tick: u64
kind: VmTimerKind pub(crate) fn snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
pub(crate) struct VmResourceSnapshot owner: VmProcessId kind: String
label: String transfer_policy: VmResourceTransferPolicy
pub(crate) fn snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process.rs",
            r#"
pub(crate) struct VmProcess mailbox: VecDeque<VmMessage>
resource_handles: Vec<String> mailbox_len selective_receive
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state_test.rs",
            r#"
vm_distributed_state_exports_and_imports_deterministic_snapshots
VmDistributedStateStore::import_snapshot(snapshot)
restored.export_snapshot()
snapshot contains duplicate state scope
snapshot scope must be valid
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_detects_corrupt_snapshot_checksum
vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark
vm_distributed_storage_recovered_snapshots_preserve_sequence_watermark
VmDistributedStorageOutcome::SnapshotLoaded
repair_snapshot
reject_replay
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process_test.rs",
            r#"
process_selective_receive_preserves_skipped_messages
process_exit_clears_mailbox_and_returns_resource_handles
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer_test.rs",
            r#"
timer_table_starts_one_shot_timer_and_exposes_snapshot
timer_table_receive_timeout_wakes_blocked_process
"#,
        )?;
        self.write(
            "std/vm/DistributedStateTest.terl",
            r#"
checkpoint_restore_is_typed
store.export_snapshot()
DistributedState.restore(snapshot)
"#,
        )?;
        self.write(
            "std/vm/DistributedStorageTest.terl",
            r#"
loaded = adapter.load_snapshot("checkpoint-b")
loaded_kind = DistributedStorage.kind(loaded)
assert_equal("snapshot_loaded", loaded_kind)
"#,
        )?;
        self.write(
            "std/vm/PersistentActorTest.terl",
            r#"
RedactionPolicy
redaction_policy_is_source_visible
PersistentActor.redaction_policy(
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
vm-persistent-actor-restore-check: vm-persistent-actor-compaction-check
	$(MAKE) vm-distributed-state-check
	$(MAKE) vm-timer-primitives-check
	$(MAKE) vm-resource-ownership-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_accepts_deterministic_export_plan -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_rejects_corrupt_export_and_stale_schema -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_rejects_wrong_actor_and_missing_resource -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_rejects_reordered_event_suffix -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_rejects_reordered_mailbox_checkpoint -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_gates_compacted_snapshot_and_resource_adapter_support -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_rejects_incompatible_adapter_kind -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_executes_cross_adapter_restore -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_builds_cross_machine_export_format -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::parse_persistent_actor_export_command_accepts_manifest_metadata -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::persistent_actor_export_command_renders_portable_manifest_without_payloads -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::parse_persistent_actor_restore_command_accepts_validation_metadata -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::persistent_actor_restore_command_renders_validated_plan_without_payloads -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_accepts_compacted_export_with_restore_boundary -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_validates_model_sync_stream_continuity -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_rejects_missing_and_reordered_model_sync_stream -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_restore::persistent_actor_restore_test::vm_persistent_actor_restore_generates_minimal_replay_fixture_without_payloads -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_restore_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-restore
"#;

#[test]
fn vm_persistent_actor_restore_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_restore(repo.root()).expect("quality check");

    assert_eq!(summary.export_manifest_count, 8);
    assert_eq!(summary.redaction_decision_count, 6);
    assert_eq!(summary.restore_validation_trace_count, 23);
    assert_eq!(summary.rejected_restore_case_count, 0);
    assert_eq!(summary.cross_adapter_restore_result_count, 6);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-restore-report-v1"));
    assert!(report.contains("persistent actor export checksum is verified before restore"));
    assert!(report.contains("distributed state export manifest"));
    assert!(report.contains("public persistent actor restore command validates restore plan"));
    assert!(report.contains("public persistent actor export command"));
    assert!(report.contains("persistent actor payload-redacted replay fixture"));
    assert!(report.contains("persistent actor restore rejects reordered mailbox checkpoint"));
    assert!(report.contains("persistent actor restore rejects incompatible adapter kind"));
    assert!(
        report.contains("persistent actor restore executes explicit cross-adapter store restore")
    );
    assert!(report.contains("persistent actor restore rejects destination store conflict"));
    assert!(report
        .contains("persistent actor cross-machine export rejects non-portable source machine id"));
    assert!(report.contains("cross-machine persistent actor export envelope"));
    assert!(report.contains("persistent actor restore accepts compacted export boundary metadata"));
    assert!(report.contains("persistent actor restore validates model-sync stream continuity"));
    assert!(report.contains(
        "persistent actor restore rejects missing or reordered model-sync stream continuity"
    ));
    assert!(!report.contains("minimal actor replay fixture generator"));
    assert!(!report.contains("reordered mailbox checkpoint restore validation"));
    assert!(!report.contains("restore into incompatible adapter"));
    assert!(!report.contains("restore after compaction API"));
    assert!(!report.contains("model-sync stream continuity validation"));
    assert!(!report.contains("cross-adapter restore execution"));
    assert!(!report.contains("cross-machine actor export format"));
    assert!(report.contains("source-visible persistent actor redaction policy descriptor is typed"));
    assert!(!report.contains("redaction policy syntax"));
    assert!(!report.contains("wrong actor owner restore validation"));
    assert!(report.contains("force-local load after reopen preserves checkpoint"));
    assert!(report.contains(
        "actor export restored from embedded key/value source into database-backed store"
    ));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_restore_rejects_missing_state_restore_anchor() {
    let repo = TestRepo::new("missing-state-restore").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_state.rs");
    let source = fs::read_to_string(&path).expect("state source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_state.rs",
        &source.replace("import_snapshot", ""),
    )
    .expect("rewrite state source");

    let error = run_vm_persistent_actor_restore(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("import_snapshot"));
}

#[test]
fn vm_persistent_actor_restore_rejects_missing_timer_inspection_anchor() {
    let repo = TestRepo::new("missing-timer-inspection").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/timer.rs");
    let source = fs::read_to_string(&path).expect("timer source");
    repo.write(
        "crates/terlan/src/runtime/vm/timer.rs",
        &source.replace("deadline_tick: u64", ""),
    )
    .expect("rewrite timer source");

    let error = run_vm_persistent_actor_restore(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("deadline_tick"));
}

#[test]
fn vm_persistent_actor_restore_rejects_missing_stale_schema_anchor() {
    let repo = TestRepo::new("missing-stale-schema").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_restore.rs");
    let source = fs::read_to_string(&path).expect("persistent actor restore source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_restore.rs",
        &source.replace("StaleSchema", ""),
    )
    .expect("rewrite persistent actor restore source");

    let error = run_vm_persistent_actor_restore(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("StaleSchema"));
}

#[test]
fn vm_persistent_actor_restore_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) vm-timer-primitives-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_restore(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-timer-primitives-check"));
}

#[test]
fn vm_persistent_actor_restore_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor restore report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "restore validation traces",
        &["todo restore validation trace"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
