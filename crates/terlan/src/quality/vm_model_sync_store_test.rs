use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_model_sync_store, validate_entries_for_placeholder_terms,
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
            "terlan-vm-model-sync-store-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/actor.rs",
            r#"
pub(crate) struct VmActorRuntime spawn_root spawn_child register_name
send_named receive_next_or_block selective_receive_or_block receive_with_timeout
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state.rs",
            r#"
pub(crate) enum VmDistributedStatePolicy
pub(crate) struct VmDistributedStateScope
pub(crate) struct VmDistributedStateVersion
pub(crate) struct VmDistributedStateEntry
pub(crate) struct VmDistributedStateConflict
pub(crate) enum VmDistributedStateWriteOutcome
write( export_snapshot import_snapshot Replayed Conflict PolicyMismatch
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
pub(crate) enum VmDistributedStorageMode
pub(crate) enum VmDistributedStorageOperation
pub(crate) struct VmDistributedStoragePolicy
pub(crate) struct VmDistributedStorageSnapshot
pub(crate) enum VmDistributedStorageOutcome
pub(crate) struct VmDistributedStorageAdapter
PartialWrite FlushTimedOut StaleSnapshot requires_recovery recovery_action
replicate_snapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/model_sync.rs",
            r#"
pub(crate) struct VmModelSyncKey
pub(crate) struct VmModelSyncVersion
pub(crate) enum VmModelSyncChangeKind
pub(crate) struct VmModelSyncChange
pub(crate) struct VmModelSyncRow
pub(crate) enum VmModelSyncOutcome
pub(crate) enum VmModelSyncProjectedFieldType
pub(crate) struct VmModelSyncRowFieldProjection
pub(crate) struct VmModelSyncRowProjection
pub(crate) struct VmModelSyncTemplateSubscription
pub(crate) struct VmModelSyncTemplateInvalidation
pub(crate) enum VmModelSyncAdapterCapability
pub(crate) struct VmModelSyncAdapterContract
pub(crate) struct VmSyncableModelDeclaration
pub(crate) enum VmModelSyncPermissionOperation
pub(crate) struct VmModelSyncFieldPermission
pub(crate) struct VmModelSyncPermissionPolicy
pub(crate) trait VmModelSyncStoreAdapter
pub(crate) struct VmInMemoryModelSyncStore
invalidate_live_template_subscribers_from_model_events
validate_non_postgres_model_sync_adapter_contracts
validate_model_sync_permission_drift
project_model_sync_row_from_adapter_fields
syncable model name must be non-empty
expected_version changes_since export_snapshot Conflict Deleted
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/native/postgres.rs",
            r#"
pub struct Pool pub fn connect pub fn query pub fn query_one
pub fn execute pub fn batch_execute pub fn transaction deadpool_postgres
build_deadpool
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/native/postgres/row.rs",
            r#"
pub struct Row enum PostgresValue pub fn string pub fn int
pub fn json pub(super) fn row_from_driver
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/native/postgres_test.rs",
            r#"
config_builders_set_pool_limits_and_timeouts
validate_config_rejects_invalid_pool_settings
query_operations_reject_empty_sql_before_adapter_dispatch
transaction_returns_stable_driver_connection_error
row_accessors_decode_matching_values
row_accessors_report_missing_and_type_errors
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/native_boundary/dispatch_test.rs",
            r#"
dispatch_postgres_query_operations_are_known_driver_operations
dispatch_postgres_transaction_requires_runtime_bridge
dispatch_postgres_row_accessors_decode_values
bridge_dispatch_postgres_row_handles_decode_values
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/db/execution_test.rs",
            "failed migration SQL cannot be followed by a committed history",
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state_test.rs",
            r#"
vm_distributed_state_reports_conflicts_with_versions_and_policy
vm_distributed_state_exports_and_imports_deterministic_snapshots
vm_distributed_state_rejects_invalid_scopes_versions_and_snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_rejects_stale_snapshot_replay
vm_distributed_storage_detects_corrupt_snapshot_checksum
vm_distributed_storage_reports_finalize_and_partial_write_failures
vm_distributed_storage_reports_flush_timeout_with_retry_recovery
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/model_sync_test.rs",
            r#"
vm_model_sync_store_applies_updates_and_exports_deterministic_snapshot
vm_model_sync_store_rejects_stale_versions_without_mutation
vm_model_sync_store_emits_delete_tombstones_and_change_streams
vm_model_sync_store_rejects_invalid_keys_and_versions
vm_model_sync_declares_syncable_model_without_orm_identity_map
vm_model_sync_rejects_invalid_syncable_model_declarations
vm_model_sync_invalidates_live_template_subscribers_from_committed_events
vm_model_sync_rejects_invalid_live_template_subscription_identity
vm_model_sync_projects_adapter_row_into_typed_model_row
vm_model_sync_row_projection_rejects_missing_adapter_field
vm_model_sync_row_projection_rejects_type_mismatch
vm_model_sync_row_projection_rejects_invalid_version_sequence
vm_model_sync_row_projection_rejects_duplicate_model_fields
vm_model_sync_permission_policy_accepts_allowed_model_and_field_changes
vm_model_sync_permission_policy_rejects_missing_model_policy
vm_model_sync_permission_policy_rejects_field_level_drift
vm_model_sync_permission_policy_rejects_denied_delete_operation
vm_model_sync_validates_non_postgres_adapter_portability_contracts
vm_model_sync_rejects_non_postgres_adapter_missing_portable_capability
vm_model_sync_rejects_non_postgres_adapter_leaking_postgres_capability
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
force_local_adapter_writes_flushes_and_loads_checkpoint
reopen_preserves_snapshots_and_sequence_watermark
assert_equal("stale_snapshot", stale_kind)
"#,
        )?;
        self.write(
            "std/vm/ModelSyncTest.terl",
            r#"
optimistic_write_plan_is_source_visible
persistent_actor_adapter_is_source_visible
package_store_adapter_is_source_visible
expected_version
next_version
ModelSync.conflict
ModelSync.adapter_contract
ModelSync.persistent_actor_adapter
ModelSync.package_store_adapter
SyncableModel
syncable_model_declaration_is_source_visible
"#,
        )?;
        self.write(
            "std/http/LiveChannelTest.terl",
            r#"
Router.sse Sse.endpoint_with_keep_alive
live_channel_sse_handler_records_queued_events
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/sse.rs",
            "VmSseEndpointPlan VmSseStream flush_next",
        )?;
        self.write(
            "crates/terlan/src/commands/serve/watch.rs",
            "ReloadHub broadcast_reload subscribers.retain",
        )?;
        self.write(
            "std/test/StatefulPropertyTest.terl",
            "StatefulPropertyTest property checks can exercise for_all property",
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
vm-model-sync-store-check: vm-web-route-schema-client-check
	$(MAKE) native-boundary-postgres-check
	$(MAKE) db-command-check
	$(TERLC) test std/vm/ModelSyncTest.terl
	bash scripts/run_exact_cargo_test.sh -p terlan formal_pipeline::formal_pipeline_test::embedded_std_interfaces_include_vm_model_sync_contract -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::model_sync::model_sync_test::vm_model_sync_store_rejects_stale_versions_without_mutation -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::model_sync::model_sync_test::vm_model_sync_projects_adapter_row_into_typed_model_row -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::model_sync::model_sync_test::vm_model_sync_permission_policy_rejects_field_level_drift -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_model_sync_store_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-model-sync-store
"#;

#[test]
fn vm_model_sync_store_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_model_sync_store(repo.root()).expect("quality check");

    assert_eq!(summary.model_fixture_count, 7);
    assert_eq!(summary.adapter_matrix_count, 9);
    assert_eq!(summary.version_conflict_case_count, 7);
    assert_eq!(summary.rejected_model_sync_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-model-sync-store-report-v1"));
    assert!(report.contains("actor-backed state fixture"));
    assert!(report.contains("model sync fixture"));
    assert!(report.contains("Postgres row fixture"));
    assert!(report.contains(
        "non-Postgres portability: typed adapter contracts reject missing portable capabilities"
    ));
    assert!(report.contains("model-sync permission policies reject model and field-level drift"));
    assert!(report.contains("row-to-model generation projects typed adapter rows into sync rows"));
    assert!(report
        .contains("Terlan-facing optimistic concurrency API builds expected and next versions"));
    assert!(
        report.contains("persistent-actor-store: source-visible persistent actor adapter binding")
    );
    assert!(report.contains("package-store: source-visible package store adapter binding"));
    assert!(report
        .contains("syncable-model: source-visible typed model declaration without ORM behavior"));
    assert!(!report.contains("public syncable model declaration syntax"));
    assert!(report.contains("committed model events invalidate typed live-template subscribers"));
    assert!(!report.contains("persistent actor store integration"));
    assert!(!report.contains("package store integration"));
    assert!(!report.contains("live-template subscriber invalidation from committed model events"));
    assert!(!report.contains("cross-adapter portability checks for non-Postgres stores"));
    assert!(!report.contains("model permissions and field-level permission drift checks"));
    assert!(!report.contains("generic store adapter trait surface"));
    assert!(!report.contains("database row to model code generation"));
    assert!(!report.contains("optimistic concurrency API exposed to Terlan code"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_model_sync_store_rejects_missing_typed_row_decoding_anchor() {
    let repo = TestRepo::new("missing-row").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/native/postgres/row.rs");
    let source = fs::read_to_string(&path).expect("row source");
    repo.write(
        "crates/terlan/src/runtime/native/postgres/row.rs",
        &source.replace("pub(super) fn row_from_driver", ""),
    )
    .expect("rewrite row source");

    let error = run_vm_model_sync_store(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("row_from_driver"));
}

#[test]
fn vm_model_sync_store_rejects_missing_storage_recovery_anchor() {
    let repo = TestRepo::new("missing-recovery").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_storage.rs");
    let source = fs::read_to_string(&path).expect("storage source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &source.replace("PartialWrite", ""),
    )
    .expect("rewrite storage source");

    let error = run_vm_model_sync_store(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("PartialWrite"));
}

#[test]
fn vm_model_sync_store_rejects_missing_live_template_propagation_anchor() {
    let repo = TestRepo::new("missing-live-template-propagation").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("std/http/LiveChannelTest.terl");
    let source = fs::read_to_string(&path).expect("live channel source");
    repo.write(
        "std/http/LiveChannelTest.terl",
        &source.replace("live_channel_sse_handler_records_queued_events", ""),
    )
    .expect("rewrite live channel source");

    let error = run_vm_model_sync_store(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("live_channel_sse_handler_records_queued_events"));
}

#[test]
fn vm_model_sync_store_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) native-boundary-postgres-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_model_sync_store(repo.root()).expect_err("gate should fail");

    assert!(error.contains("native-boundary-postgres-check"));
}

#[test]
fn vm_model_sync_store_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM model sync store report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms("model fixtures", &["todo sync fixture"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
