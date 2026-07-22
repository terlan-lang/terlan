use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_adapter, validate_entries_for_placeholder_terms,
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
            "terlan-vm-persistent-actor-adapter-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/persistent_actor_adapter.rs",
            r#"
VmPersistentActorAdapterCapabilityManifest
VmPersistentActorAdapterConformanceFixture
VmPersistentActorAdapterConformanceReport
VmPersistentActorAdapterConformanceError
plan_persistent_actor_adapter_conformance
MissingClusterReplicationCapability
StorageOutcomeRejected
ReplayDiverged
require_compare_and_swap
stale_compare_and_swap_token_for_test
local_persistent_actor_adapter_fixture
cluster_persistent_actor_adapter_fixture
file_backed_persistent_actor_adapter_fixture
database_backed_persistent_actor_adapter_fixture
embedded_key_value_persistent_actor_adapter_fixture
package_provided_persistent_actor_adapter_fixture
execute_persistent_actor_adapter_cross_adapter_restore
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_adapter_test.rs",
            r#"
vm_persistent_actor_adapter_conformance_accepts_local_fixture_replay
vm_persistent_actor_adapter_conformance_accepts_cluster_replication_fixture
vm_persistent_actor_adapter_conformance_accepts_file_backed_fixture_replay
vm_persistent_actor_adapter_conformance_accepts_database_backed_fixture_replay
vm_persistent_actor_adapter_conformance_accepts_embedded_key_value_fixture_replay
vm_persistent_actor_adapter_conformance_accepts_package_provided_fixture_replay
vm_persistent_actor_adapter_conformance_executes_cross_adapter_restore
vm_persistent_actor_adapter_conformance_rejects_missing_cluster_capability
vm_persistent_actor_adapter_conformance_rejects_unavailable_adapter
vm_persistent_actor_adapter_conformance_rejects_corrupt_and_partial_checkpoints
vm_persistent_actor_adapter_conformance_rejects_stale_replay_sequence
vm_persistent_actor_adapter_conformance_rejects_stale_compare_and_swap_token
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
pub(crate) enum VmDistributedStorageMode LocalOnly Durable Cluster
pub(crate) enum VmDistributedStorageOperation ClusterReplicate CompareAndSwapAppend SnapshotIsolation DurableFlush TransactionalBatchAppend SchemaMigration ResourceHandleValidation
VmDistributedStorageAtomicAppendProof VmDistributedStorageSnapshotIsolationProof VmDistributedStorageDurableFlushProof VmDistributedStorageTransactionalBatchProof VmDistributedStorageSchemaMigrationProof VmDistributedStorageResourceHandleValidationProof
VmDistributedStorageCasToken CompareAndSwapTokenMismatch SchemaMigrationMismatch ResourceHandleValidationFailed
pub(crate) struct VmDistributedStoragePolicy can_cluster_replicate
supports(&self, operation pub(crate) struct VmDistributedStorageAdapter
require_atomic_append atomic_append_proof observed_sequence
require_snapshot_isolation snapshot_isolation_proof checkpoint_id(&self)
require_durable_flush durable_flush_proof flushed_sequence
require_transactional_batch transactional_batch_proof transactional_batch_append committed_count
require_schema_migration schema_migration_proof migrate_schema schema_version expected_schema actual_schema
require_resource_handle_validation resource_handle_validation_proof register_resource_handle validate_resource_handles missing_resource_handle validated_resource_count
compare_and_swap_token compare_and_swap_append replicate_snapshot
require_cluster_replication expected_sequence actual_sequence StorageUnavailable Unsupported
PartialWrite ChecksumMismatch StaleSnapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_force_local_writes_flushes_and_loads_snapshot
vm_distributed_storage_durable_mode_writes_flushes_and_loads_snapshot
vm_distributed_storage_cluster_capability_requires_cluster_mode_and_availability
vm_distributed_storage_cluster_mode_writes_flushes_and_loads_snapshot
vm_distributed_storage_cluster_mode_replicates_snapshots_through_adapter
vm_distributed_storage_cluster_replication_reports_typed_failures
vm_distributed_storage_atomic_append_proof_preserves_sequence_on_failed_append
vm_distributed_storage_snapshot_isolation_proof_survives_later_compaction
vm_distributed_storage_durable_flush_proof_advances_only_after_successful_flush
vm_distributed_storage_transactional_batch_rejects_partial_commit_without_mutation
vm_distributed_storage_schema_migration_rejects_stale_expected_version
vm_distributed_storage_resource_handle_validation_rejects_missing_handles_without_mutation
vm_distributed_storage_compare_and_swap_append_rejects_stale_token
vm_distributed_storage_returns_unavailable_for_missing_backend_without_panics
vm_distributed_storage_reports_finalize_and_partial_write_failures
vm_distributed_storage_detects_corrupt_snapshot_checksum
vm_distributed_storage_rejects_stale_snapshot_replay
"#,
        )?;
        self.write(
            "std/vm/DistributedStorageTest.terl",
            r#"
force_local_adapter_writes_flushes_and_loads_checkpoint
closed_adapter_lifecycle_failures_are_typed
cluster_adapter_capability_is_explicit
policy_can_cluster_replicate
atomic_append_contract_is_source_visible
adapter.require_atomic_append()
adapter.atomic_append_proof()
DistributedStorage.proof_sequence
snapshot_isolation_contract_is_source_visible
adapter.require_snapshot_isolation()
adapter.snapshot_isolation_proof
DistributedStorage.isolation_checkpoint_id
DistributedStorage.isolation_sequence
DistributedStorage.isolation_checksum
durable_flush_contract_is_source_visible
adapter.require_durable_flush()
adapter.durable_flush_proof()
DistributedStorage.durable_flush_sequence
transactional_batch_contract_is_source_visible
adapter.require_transactional_batch()
adapter.transactional_batch_proof()
adapter.transactional_batch_append([first, second])
DistributedStorage.batch_committed_count
schema_migration_contract_is_source_visible
adapter.require_schema_migration()
adapter.schema_migration_proof()
adapter.migrate_schema(0, 2)
DistributedStorage.schema_version
resource_handle_validation_contract_is_source_visible
adapter.require_resource_handle_validation()
adapter.resource_handle_validation_proof()
adapter.validate_resource_handles(["db.primary"])
DistributedStorage.resource_handle_count
adapter.require_cluster_replication()
adapter.replicate_snapshot(snapshot)
compare_and_swap_append_contract_is_source_visible
adapter.compare_and_swap_token()
adapter.compare_and_swap_append(snapshot, token)
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
vm-persistent-actor-adapter-conformance-check: vm-persistent-actor-restore-check
	$(MAKE) vm-distributed-state-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_accepts_local_fixture_replay -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_accepts_cluster_replication_fixture -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_accepts_file_backed_fixture_replay -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_accepts_database_backed_fixture_replay -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_accepts_embedded_key_value_fixture_replay -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_accepts_package_provided_fixture_replay -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_executes_cross_adapter_restore -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_rejects_missing_cluster_capability -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_rejects_unavailable_adapter -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_rejects_corrupt_and_partial_checkpoints -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_rejects_stale_replay_sequence -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_adapter::persistent_actor_adapter_test::vm_persistent_actor_adapter_conformance_rejects_stale_compare_and_swap_token -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_atomic_append_proof_preserves_sequence_on_failed_append -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_snapshot_isolation_proof_survives_later_compaction -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_durable_flush_proof_advances_only_after_successful_flush -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_transactional_batch_rejects_partial_commit_without_mutation -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_schema_migration_rejects_stale_expected_version -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_resource_handle_validation_rejects_missing_handles_without_mutation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_adapter_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-adapter
"#;

#[test]
fn vm_persistent_actor_adapter_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_adapter(repo.root()).expect("quality check");

    assert_eq!(summary.adapter_capability_manifest_count, 18);
    assert_eq!(summary.conformance_matrix_count, 23);
    assert_eq!(summary.crash_injection_outcome_count, 14);
    assert_eq!(summary.rejected_adapter_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-adapter-report-v1"));
    assert!(report.contains("force-local: open, append, flush, compact, load, close"));
    assert!(report.contains(
        "persistent actor conformance manifest: adapter name, mode, operations, cluster replication"
    ));
    assert!(report
        .contains("atomic append proof: capability, observed sequence, partial-write isolation"));
    assert!(report.contains("atomic append proof does not advance across failed append attempts"));
    assert!(report.contains(
        "snapshot isolation proof: capability, checkpoint id, sequence, checksum, compaction isolation"
    ));
    assert!(
        report.contains("snapshot isolation proof survives later adapter append and compaction")
    );
    assert!(report
        .contains("durable flush proof: capability, flushed sequence, failed-flush isolation"));
    assert!(report.contains("durable flush proof advances only after successful flush"));
    assert!(report.contains("durable flush proof exposes last successful flush sequence"));
    assert!(report.contains(
        "transactional batch append: capability, all-or-nothing commit, committed-count proof"
    ));
    assert!(report.contains("transactional batch append rejects partial commit without mutation"));
    assert!(report.contains(
        "transactional batch partial commit returns rewrite_checkpoint without mutation"
    ));
    assert!(report.contains(
        "transactional batch proof exposes first sequence, last sequence, and committed count"
    ));
    assert!(report.contains("schema migration: capability, expected-version guard, schema proof"));
    assert!(report.contains("schema migration rejects stale expected version without mutation"));
    assert!(report.contains("schema migration mismatch returns reload_schema before migration"));
    assert!(report.contains("schema migration proof exposes schema version and storage sequence"));
    assert!(report
        .contains("resource handle validation: capability, registered handles, validation proof"));
    assert!(report.contains("resource handle validation rejects missing handles without mutation"));
    assert!(report
        .contains("resource handle validation returns recover_resource_handle before restore"));
    assert!(report
        .contains("resource handle validation proof exposes validated count and storage sequence"));
    assert!(report.contains("compare-and-swap append: token, guarded append, stale-token recovery"));
    assert!(report
        .contains("persistent actor cluster fixture uses explicit cluster replication capability"));
    assert!(report.contains("persistent actor local fixture uses compare-and-swap append tokens"));
    assert!(report.contains(
        "file-backed persistent actor adapter: durable append, flush, load, compact, and close"
    ));
    assert!(report.contains(
        "persistent actor file-backed fixture replays through durable adapter semantics"
    ));
    assert!(
        report.contains("file-backed durable fixture preserves replay after flush and compaction")
    );
    assert!(report.contains("file-backed durable checkpoint fixture replays"));
    assert!(report.contains(
        "database-backed persistent actor adapter: durable transaction log replay and compaction"
    ));
    assert!(report.contains(
        "persistent actor database-backed fixture replays through durable adapter semantics"
    ));
    assert!(report.contains(
        "database-backed durable fixture preserves ordered replay after flush and compaction"
    ));
    assert!(report.contains("database-backed durable checkpoint fixture replays"));
    assert!(report.contains(
        "embedded key/value persistent actor adapter: deterministic key/value replay and compaction"
    ));
    assert!(report.contains(
        "persistent actor embedded key/value fixture replays through durable adapter semantics"
    ));
    assert!(report.contains(
        "embedded key/value durable fixture preserves replay after flush and compaction"
    ));
    assert!(report.contains("embedded key/value durable checkpoint fixture replays"));
    assert!(report.contains(
        "package-provided persistent actor adapter: generated package manifest replay and compaction"
    ));
    assert!(report.contains(
        "persistent actor package-provided fixture replays through durable adapter semantics"
    ));
    assert!(report
        .contains("package-provided durable fixture preserves replay after flush and compaction"));
    assert!(report.contains("package-provided durable checkpoint fixture replays"));
    assert!(report.contains(
        "persistent actor cross-adapter restore executes through the shared adapter contract"
    ));
    assert!(
        report.contains("cross-adapter restore preserves source and destination adapter metadata")
    );
    assert!(report.contains("cross-adapter persistent actor restore fixture replays"));
    assert!(report.contains(
        "persistent actor stale compare-and-swap token returns reload_snapshot before append"
    ));
    assert!(report
        .contains("persistent actor corrupt checkpoint returns repair_snapshot before replay"));
    assert!(!report.contains("\"file-backed persistent actor adapter\""));
    assert!(!report.contains("\"database-backed persistent actor adapter\""));
    assert!(!report.contains("\"embedded key/value persistent actor adapter\""));
    assert!(!report.contains("\"package-provided persistent actor adapter\""));
    assert!(!report.contains("\"cross-adapter restore execution\""));
    assert!(!report.contains("atomic append capability proof"));
    assert!(!report.contains("snapshot isolation capability proof"));
    assert!(!report.contains("fsync or equivalent durability proof"));
    assert!(!report.contains("transactional batch support"));
    assert!(!report.contains("schema migration through adapter API"));
    assert!(!report.contains("resource handle validation through adapter API"));
    assert!(!report.contains("compare-and-swap token contract"));
    assert!(report.contains("cluster replicated checkpoint fixture replays"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_adapter_rejects_missing_capability_anchor() {
    let repo = TestRepo::new("missing-capability").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_storage.rs");
    let source = fs::read_to_string(&path).expect("storage source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &source.replace("can_cluster_replicate", ""),
    )
    .expect("rewrite storage source");

    let error = run_vm_persistent_actor_adapter(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("can_cluster_replicate"));
}

#[test]
fn vm_persistent_actor_adapter_rejects_missing_cluster_test_anchor() {
    let repo = TestRepo::new("missing-cluster-test").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_storage_test.rs");
    let source = fs::read_to_string(&path).expect("storage test source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &source.replace(
            "vm_distributed_storage_cluster_mode_replicates_snapshots_through_adapter",
            "",
        ),
    )
    .expect("rewrite storage test source");

    let error = run_vm_persistent_actor_adapter(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("cluster_mode_replicates_snapshots"));
}

#[test]
fn vm_persistent_actor_adapter_rejects_missing_cluster_capability_error_anchor() {
    let repo = TestRepo::new("missing-cluster-capability-error").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_adapter.rs");
    let source = fs::read_to_string(&path).expect("persistent actor adapter source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_adapter.rs",
        &source.replace("MissingClusterReplicationCapability", ""),
    )
    .expect("rewrite persistent actor adapter source");

    let error = run_vm_persistent_actor_adapter(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("MissingClusterReplicationCapability"));
}

#[test]
fn vm_persistent_actor_adapter_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) vm-distributed-state-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_adapter(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-distributed-state-check"));
}

#[test]
fn vm_persistent_actor_adapter_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor adapter report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected =
        validate_entries_for_placeholder_terms("conformance matrix", &["todo adapter fixture"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
