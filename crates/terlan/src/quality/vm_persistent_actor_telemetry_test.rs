use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_telemetry, validate_entries_for_placeholder_terms,
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
            "terlan-vm-persistent-actor-telemetry-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
VmDistributedStorageOutcome SnapshotLoaded ChecksumMismatch PartialWrite
FlushTimedOut StaleSnapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/coordination.rs",
            r#"
trace_id VmCoordinationEnvelope VmDistributedTransportFrame
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/code_server.rs",
            r#"
event_snapshots VmCodeServerEventSnapshot source_map_id
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/failure.rs",
            r#"
VmFailureReport delivered_exit_signals delivered_down_messages
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process.rs",
            r#"
mailbox: VecDeque<VmMessage> resource_handles mailbox_len
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer.rs",
            r#"
VmTimerSnapshot owner: VmProcessId deadline_tick: u64
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
VmResourceSnapshot owner: VmProcessId transfer_policy
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_telemetry.rs",
            r#"
VmPersistentActorTelemetrySpan validate_persistent_actor_telemetry_trace
MisleadingSuccessAfterFailure UnredactedSecret deterministic_restore_trace
VmPersistentActorTelemetryCollector VmPersistentActorTelemetryLimits
VmPersistentActorTelemetryLifecycle
VmPersistentActorDebuggerHandoff persistent_actor_debugger_handoff
VmPersistentActorTelemetrySupportPolicy VmPersistentActorTelemetrySupportBundle
persistent_actor_telemetry_support_bundle
publish_model_sync_changes ModelSyncSequenceRegression EmptyModelSyncStream
TelemetryAfterRollback CardinalityLimitExceeded ActorIdentityMismatch CounterOverflow
MissingSourceMapIdentity ReplayStepUnavailable
MailboxRestore TimerRestore PostRecoveryMessage
"#,
        )?;
        self.write(
            "std/vm/Fault.terl",
            r#"
classification rollback descriptor pub failure(rollback: Rollback)
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_telemetry_aggregation.rs",
            r#"
VmPersistentActorMetricAggregator VmPersistentActorMetricLimits
VmPersistentActorMetricSeries CardinalityLimitExceeded CounterOverflow ingest_trace
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_reports_finalize_and_partial_write_failures
vm_distributed_storage_detects_corrupt_snapshot_checksum
vm_distributed_storage_rejects_stale_snapshot_replay
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/coordination_test.rs",
            r#"
trace:vm-a:vm-b:1
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/code_server_test.rs",
            r#"
source_hot_reload_records_reload_and_rollback_events_for_inspection
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/failure_test.rs",
            r#"
failure_runtime_reports_missing_exited_and_self_link_diagnostics
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_telemetry_test.rs",
            r#"
vm_persistent_actor_telemetry_accepts_deterministic_restore_trace
vm_persistent_actor_telemetry_preserves_typed_failure_classification
vm_persistent_actor_telemetry_rejects_duplicate_and_out_of_order_spans
vm_persistent_actor_telemetry_rejects_missing_identity_and_bad_ranges
vm_persistent_actor_telemetry_rejects_secret_leak_and_success_after_failure
vm_persistent_actor_telemetry_collector_emits_operation_spans_with_redaction
vm_persistent_actor_telemetry_collector_propagates_failure_and_stops_after_rollback
vm_persistent_actor_telemetry_collector_enforces_cardinality_limits
vm_persistent_actor_telemetry_rejects_mixed_identity_and_counter_overflow
vm_persistent_actor_telemetry_lifecycle_emits_store_and_restore_spans
vm_persistent_actor_telemetry_lifecycle_rejects_identity_drift_and_traces_failures
vm_persistent_actor_telemetry_builds_typed_debugger_handoff
vm_persistent_actor_telemetry_rejects_invalid_debugger_handoff
vm_persistent_actor_telemetry_exports_structurally_redacted_support_bundle
vm_persistent_actor_telemetry_support_bundle_rejects_invalid_trace
vm_persistent_actor_telemetry_publishes_ordered_model_sync_stream
vm_persistent_actor_telemetry_rejects_invalid_model_sync_stream_atomically
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_telemetry_aggregation_test.rs",
            r#"
vm_persistent_actor_metrics_aggregate_cross_actor_without_actor_id_labels
vm_persistent_actor_metrics_reject_limits_and_overflow_atomically
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
vm-persistent-actor-telemetry-check: vm-persistent-actor-performance-budget-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_accepts_deterministic_restore_trace -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_preserves_typed_failure_classification -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_rejects_duplicate_and_out_of_order_spans -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_rejects_missing_identity_and_bad_ranges -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_rejects_secret_leak_and_success_after_failure -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_collector_emits_operation_spans_with_redaction -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_collector_propagates_failure_and_stops_after_rollback -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_collector_enforces_cardinality_limits -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_rejects_mixed_identity_and_counter_overflow -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_lifecycle_emits_store_and_restore_spans -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_lifecycle_rejects_identity_drift_and_traces_failures -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_builds_typed_debugger_handoff -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_rejects_invalid_debugger_handoff -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_exports_structurally_redacted_support_bundle -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_support_bundle_rejects_invalid_trace -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_publishes_ordered_model_sync_stream -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry::persistent_actor_telemetry_test::vm_persistent_actor_telemetry_rejects_invalid_model_sync_stream_atomically -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry_aggregation::persistent_actor_telemetry_aggregation_test::vm_persistent_actor_metrics_aggregate_cross_actor_without_actor_id_labels -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_telemetry_aggregation::persistent_actor_telemetry_aggregation_test::vm_persistent_actor_metrics_reject_limits_and_overflow_atomically -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_telemetry_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-telemetry
"#;

#[test]
fn vm_persistent_actor_telemetry_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_telemetry(repo.root()).expect("quality check");

    assert_eq!(summary.trace_fixture_count, 7);
    assert_eq!(summary.span_schema_field_count, 11);
    assert_eq!(summary.deterministic_trace_validation_count, 19);
    assert_eq!(summary.replay_timeline_count, 6);
    assert_eq!(summary.rejected_telemetry_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-telemetry-report-v1"));
    assert!(report.contains("typed_failure_reason"));
    assert!(!report.contains("support bundle redaction policy"));
    assert!(report.contains("deliver first post-recovery message"));
    assert!(report.contains("deterministic restore replay timeline"));
    assert!(report.contains("secret leak and success-after-failure rejection"));
    assert!(report.contains("typed failure propagation and post-rollback suppression"));
    assert!(report.contains("typed debugger handoff from a validated replay step"));
    assert!(report.contains("structurally redacted support bundle export"));
    assert!(report.contains("ordered model sync stream publication without row payload leakage"));
    assert!(report.contains("bounded cross actor aggregation without actor id metric labels"));
    assert!(!report.contains("cross-actor metric cardinality aggregation"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_telemetry_rejects_missing_trace_anchor() {
    let repo = TestRepo::new("missing-trace").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/coordination.rs");
    let source = fs::read_to_string(&path).expect("coordination source");
    repo.write(
        "crates/terlan/src/runtime/vm/coordination.rs",
        &source.replace("trace_id", ""),
    )
    .expect("rewrite coordination source");

    let error = run_vm_persistent_actor_telemetry(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("trace_id"));
}

#[test]
fn vm_persistent_actor_telemetry_rejects_missing_failure_fixture_anchor() {
    let repo = TestRepo::new("missing-failure-fixture").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/failure_test.rs");
    let source = fs::read_to_string(&path).expect("failure test source");
    repo.write(
        "crates/terlan/src/runtime/vm/failure_test.rs",
        &source.replace(
            "failure_runtime_reports_missing_exited_and_self_link_diagnostics",
            "",
        ),
    )
    .expect("rewrite failure test source");

    let error = run_vm_persistent_actor_telemetry(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("self_link_diagnostics"));
}

#[test]
fn vm_persistent_actor_telemetry_rejects_missing_post_recovery_anchor() {
    let repo = TestRepo::new("missing-post-recovery").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_telemetry.rs");
    let source = fs::read_to_string(&path).expect("persistent actor telemetry source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_telemetry.rs",
        &source.replace("PostRecoveryMessage", ""),
    )
    .expect("rewrite persistent actor telemetry source");

    let error = run_vm_persistent_actor_telemetry(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("PostRecoveryMessage"));
}

#[test]
fn vm_persistent_actor_telemetry_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("vm_persistent_actor_telemetry_test", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_telemetry(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm_persistent_actor_telemetry_test"));
}

#[test]
fn vm_persistent_actor_telemetry_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor telemetry report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "trace fixtures",
        &["todo persistent actor telemetry fixture"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
