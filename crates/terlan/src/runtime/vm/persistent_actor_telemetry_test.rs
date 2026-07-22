use super::super::model_sync::{
    VmModelSyncChange, VmModelSyncChangeKind, VmModelSyncKey, VmModelSyncVersion,
};
use super::super::persistent_actor_store::{
    VmInMemoryPersistentActorStore, VmPersistentActorEvent, VmPersistentActorId,
    VmPersistentActorSchema, VmPersistentActorSnapshot, VmPersistentActorStoreOutcome,
};
use super::super::persistent_actor_telemetry_aggregation::VmPersistentActorMetricLimits;
use super::super::ReplValue;
use super::{
    deterministic_restore_trace, persistent_actor_debugger_handoff,
    persistent_actor_telemetry_support_bundle, validate_persistent_actor_telemetry_trace,
    VmPersistentActorTelemetryCollector, VmPersistentActorTelemetryError,
    VmPersistentActorTelemetryEvent, VmPersistentActorTelemetryKind,
    VmPersistentActorTelemetryLifecycle, VmPersistentActorTelemetryLifecycleError,
    VmPersistentActorTelemetryLimits, VmPersistentActorTelemetrySupportPolicy,
};

#[test]
fn vm_persistent_actor_telemetry_builds_typed_debugger_handoff() {
    let spans = deterministic_restore_trace();

    let handoff = persistent_actor_debugger_handoff(&spans, "app.Order:source-fnv1a64:1234", 5)
        .expect("validated debugger handoff");

    assert_eq!(handoff.source_map_id, "app.Order:source-fnv1a64:1234");
    assert_eq!(handoff.replay_step, 5);
    assert_eq!(handoff.actor_id, "actor-1");
    assert_eq!(handoff.snapshot_generation, 7);
    assert_eq!(
        handoff.operation,
        VmPersistentActorTelemetryKind::ResourceValidation
    );
    assert_eq!((handoff.event_start, handoff.event_end), (16, 16));
    assert_eq!(handoff.typed_failure_reason, None);
}

#[test]
fn vm_persistent_actor_telemetry_rejects_invalid_debugger_handoff() {
    let spans = deterministic_restore_trace();

    assert_eq!(
        persistent_actor_debugger_handoff(&spans, "  ", 1),
        Err(VmPersistentActorTelemetryError::MissingSourceMapIdentity)
    );
    assert_eq!(
        persistent_actor_debugger_handoff(&spans, "app.Order:source-map", 0),
        Err(VmPersistentActorTelemetryError::ReplayStepUnavailable { replay_step: 0 })
    );

    let mut malformed = spans;
    malformed.swap(0, 1);
    assert!(matches!(
        persistent_actor_debugger_handoff(&malformed, "app.Order:source-map", 1),
        Err(VmPersistentActorTelemetryError::OutOfOrderSequence { .. })
    ));
}

#[test]
fn vm_persistent_actor_telemetry_exports_structurally_redacted_support_bundle() {
    let mut spans = deterministic_restore_trace();
    for span in &mut spans {
        span.actor_id = "customer-alice".to_string();
        span.actor_family = "private-orders".to_string();
        span.schema_id = "secret-schema".to_string();
        span.adapter_id = "postgres-password".to_string();
        span.recovery_phase = "private-recovery-token".to_string();
        span.typed_failure_reason = Some("database-password=unsafe".to_string());
    }

    let bundle = persistent_actor_telemetry_support_bundle(
        &spans,
        &VmPersistentActorTelemetrySupportPolicy::redacted(),
    )
    .expect("redacted support bundle");
    let rendered = format!("{bundle:?}");

    assert_eq!(bundle.actor_reference, "[redacted-actor]");
    assert_eq!(bundle.steps.len(), spans.len());
    assert!(bundle.failed);
    assert!(bundle.steps.iter().all(|step| step.failed));
    for secret in [
        "customer-alice",
        "private-orders",
        "secret-schema",
        "postgres-password",
        "private-recovery-token",
        "database-password=unsafe",
    ] {
        assert!(!rendered.contains(secret), "support bundle leaked {secret}");
    }
}

#[test]
fn vm_persistent_actor_telemetry_support_bundle_rejects_invalid_trace() {
    let policy = VmPersistentActorTelemetrySupportPolicy::redacted();
    assert_eq!(
        persistent_actor_telemetry_support_bundle(&[], &policy),
        Err(VmPersistentActorTelemetryError::EmptyTrace)
    );

    let mut spans = deterministic_restore_trace();
    spans[1].actor_id = "another-actor".to_string();
    assert!(matches!(
        persistent_actor_telemetry_support_bundle(&spans, &policy),
        Err(VmPersistentActorTelemetryError::ActorIdentityMismatch { sequence: 2 })
    ));
}

fn model_sync_change(sequence: u64, model: &str, id: &str) -> VmModelSyncChange {
    VmModelSyncChange {
        sequence,
        key: VmModelSyncKey {
            model: model.to_string(),
            id: id.to_string(),
        },
        version: VmModelSyncVersion {
            sequence,
            writer_id: "node-a".to_string(),
        },
        kind: VmModelSyncChangeKind::Updated,
        value: Some(ReplValue::String("private-row-value".to_string())),
    }
}

#[test]
fn vm_persistent_actor_telemetry_publishes_ordered_model_sync_stream() {
    let mut collector = VmPersistentActorTelemetryCollector::new(
        "actor-1",
        "orders",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("collector");
    let changes = [
        model_sync_change(1, "User", "alice"),
        model_sync_change(2, "Session", "session-1"),
        model_sync_change(3, "User", "bob"),
    ];

    collector
        .publish_model_sync_changes("orders-v1", 7, "model-store", &changes)
        .expect("publish model sync telemetry");

    assert_eq!(collector.spans().len(), 3);
    assert!(collector.spans().iter().all(|span| {
        span.kind == VmPersistentActorTelemetryKind::ModelSyncPublication
            && span.recovery_phase == "model_sync"
            && span.redacted_resource_label.is_none()
    }));
    assert_eq!(collector.spans()[0].event_start, 1);
    assert_eq!(collector.spans()[2].event_end, 3);
    assert!(!format!("{:?}", collector.spans()).contains("private-row-value"));
}

#[test]
fn vm_persistent_actor_telemetry_rejects_invalid_model_sync_stream_atomically() {
    let mut collector = VmPersistentActorTelemetryCollector::new(
        "actor-1",
        "orders",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("collector");
    let initial = [model_sync_change(5, "User", "alice")];
    collector
        .publish_model_sync_changes("orders-v1", 7, "model-store", &initial)
        .expect("initial stream");
    let span_count = collector.spans().len();

    assert_eq!(
        collector.publish_model_sync_changes("orders-v1", 7, "model-store", &[]),
        Err(VmPersistentActorTelemetryError::EmptyModelSyncStream)
    );
    let invalid = [
        model_sync_change(6, "User", "bob"),
        model_sync_change(0, "User", "bad"),
    ];
    assert_eq!(
        collector.publish_model_sync_changes("orders-v1", 7, "model-store", &invalid),
        Err(VmPersistentActorTelemetryError::InvalidModelSyncChange { sequence: 0 })
    );
    let regressed = [model_sync_change(5, "User", "duplicate")];
    assert_eq!(
        collector.publish_model_sync_changes("orders-v1", 7, "model-store", &regressed),
        Err(
            VmPersistentActorTelemetryError::ModelSyncSequenceRegression {
                model: "User".to_string(),
                previous: 5,
                next: 5,
            }
        )
    );
    assert_eq!(collector.spans().len(), span_count);
}

fn event(kind: VmPersistentActorTelemetryKind) -> VmPersistentActorTelemetryEvent {
    VmPersistentActorTelemetryEvent {
        kind,
        schema_id: "orders-v1".to_string(),
        snapshot_generation: 7,
        event_start: 1,
        event_end: 2,
        adapter_id: "local-durable".to_string(),
        scheduler_ticks: 3,
        durable_bytes: 128,
        retry_count: 0,
        recovery_phase: "active".to_string(),
        typed_failure_reason: None,
        resource_label: Some("database-password=unsafe".to_string()),
    }
}

#[test]
fn vm_persistent_actor_telemetry_accepts_deterministic_restore_trace() {
    let trace = validate_persistent_actor_telemetry_trace(&deterministic_restore_trace())
        .expect("telemetry trace");

    assert_eq!(trace.actor_id, "actor-1");
    assert_eq!(
        trace.replay_timeline,
        vec![
            "1:snapshot",
            "2:replay",
            "3:mailbox_restore",
            "4:timer_restore",
            "5:resource_validation",
            "6:post_recovery_message"
        ]
    );
    assert_eq!(trace.total_scheduler_ticks, 63);
    assert_eq!(trace.total_durable_bytes, 2688);
    assert_eq!(trace.failure_classification, None);
}

#[test]
fn vm_persistent_actor_telemetry_preserves_typed_failure_classification() {
    let mut spans = deterministic_restore_trace();
    for span in spans.iter_mut().skip(2) {
        span.typed_failure_reason = Some("adapter_timeout".to_string());
    }

    let trace = validate_persistent_actor_telemetry_trace(&spans).expect("telemetry trace");

    assert_eq!(
        trace.failure_classification,
        Some("adapter_timeout".to_string())
    );
}

#[test]
fn vm_persistent_actor_telemetry_rejects_duplicate_and_out_of_order_spans() {
    let mut duplicate = deterministic_restore_trace();
    duplicate[2].sequence = duplicate[1].sequence;
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&duplicate),
        Err(VmPersistentActorTelemetryError::DuplicateSequence { sequence: 2 })
    );

    let mut out_of_order = deterministic_restore_trace();
    out_of_order[2].sequence = 1;
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&out_of_order),
        Err(VmPersistentActorTelemetryError::OutOfOrderSequence {
            previous: 2,
            next: 1,
        })
    );
}

#[test]
fn vm_persistent_actor_telemetry_rejects_missing_identity_and_bad_ranges() {
    let mut missing_identity = deterministic_restore_trace();
    missing_identity[0].actor_id.clear();
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&missing_identity),
        Err(VmPersistentActorTelemetryError::MissingActorIdentity { sequence: 1 })
    );

    let mut bad_range = deterministic_restore_trace();
    bad_range[1].event_start = 20;
    bad_range[1].event_end = 10;
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&bad_range),
        Err(VmPersistentActorTelemetryError::EmptyEventRange { sequence: 2 })
    );
}

#[test]
fn vm_persistent_actor_telemetry_rejects_secret_leak_and_success_after_failure() {
    let mut secret = deterministic_restore_trace();
    secret[2].redacted_resource_label = Some("private-token-123".to_string());
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&secret),
        Err(VmPersistentActorTelemetryError::UnredactedSecret { sequence: 3 })
    );

    let mut misleading = deterministic_restore_trace();
    misleading[1].typed_failure_reason = Some("checksum_mismatch".to_string());
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&misleading),
        Err(VmPersistentActorTelemetryError::MisleadingSuccessAfterFailure { sequence: 3 },)
    );
}

#[test]
fn vm_persistent_actor_telemetry_collector_emits_operation_spans_with_redaction() {
    let mut collector = VmPersistentActorTelemetryCollector::new(
        "actor-1",
        "orders",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("collector");
    for kind in [
        VmPersistentActorTelemetryKind::Append,
        VmPersistentActorTelemetryKind::Snapshot,
        VmPersistentActorTelemetryKind::Checkpoint,
        VmPersistentActorTelemetryKind::Replay,
        VmPersistentActorTelemetryKind::SchemaMigration,
        VmPersistentActorTelemetryKind::Compaction,
        VmPersistentActorTelemetryKind::Export,
        VmPersistentActorTelemetryKind::ModelSyncPublication,
    ] {
        collector.emit(event(kind)).expect("emit telemetry");
    }

    assert_eq!(collector.spans().len(), 8);
    assert_eq!(collector.spans()[0].sequence, 1);
    assert_eq!(collector.spans()[7].sequence, 8);
    assert!(collector
        .spans()
        .iter()
        .all(|span| { span.redacted_resource_label.as_deref() == Some("[redacted-resource]") }));

    let trace = collector.finish().expect("finish trace");
    assert_eq!(trace.replay_timeline[0], "1:append");
    assert_eq!(trace.replay_timeline[7], "8:model_sync_publication");
}

#[test]
fn vm_persistent_actor_telemetry_collector_propagates_failure_and_stops_after_rollback() {
    let mut collector = VmPersistentActorTelemetryCollector::new(
        "actor-1",
        "orders",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("collector");
    let mut failure = event(VmPersistentActorTelemetryKind::AdapterFailure);
    failure.typed_failure_reason = Some("adapter_timeout".to_string());
    collector.emit(failure).expect("emit failure");
    collector
        .emit(event(VmPersistentActorTelemetryKind::Restore))
        .expect("emit recovery span");
    assert_eq!(
        collector.spans()[1].typed_failure_reason.as_deref(),
        Some("adapter_timeout")
    );
    let mut changed_failure = event(VmPersistentActorTelemetryKind::AdapterFailure);
    changed_failure.typed_failure_reason = Some("checksum_mismatch".to_string());
    assert_eq!(
        collector.emit(changed_failure),
        Err(VmPersistentActorTelemetryError::FailureClassificationChanged { sequence: 3 })
    );

    collector.complete_rollback();
    assert_eq!(
        collector.emit(event(VmPersistentActorTelemetryKind::Replay)),
        Err(VmPersistentActorTelemetryError::TelemetryAfterRollback)
    );
    assert_eq!(collector.spans().len(), 2);
}

#[test]
fn vm_persistent_actor_telemetry_collector_enforces_cardinality_limits() {
    let limits = VmPersistentActorTelemetryLimits {
        schema_ids: 1,
        adapter_ids: 1,
        failure_reasons: 1,
    };
    let mut collector =
        VmPersistentActorTelemetryCollector::new("actor-1", "orders", limits).expect("collector");
    collector
        .emit(event(VmPersistentActorTelemetryKind::Append))
        .expect("first schema");

    let mut second_schema = event(VmPersistentActorTelemetryKind::Snapshot);
    second_schema.schema_id = "orders-v2".to_string();
    assert_eq!(
        collector.emit(second_schema),
        Err(VmPersistentActorTelemetryError::CardinalityLimitExceeded {
            dimension: "schema_id",
            limit: 1,
        })
    );

    let mut adapter_collector =
        VmPersistentActorTelemetryCollector::new("actor-1", "orders", limits)
            .expect("adapter collector");
    adapter_collector
        .emit(event(VmPersistentActorTelemetryKind::Append))
        .expect("first adapter");
    let mut second_adapter = event(VmPersistentActorTelemetryKind::Snapshot);
    second_adapter.adapter_id = "remote-durable".to_string();
    assert_eq!(
        adapter_collector.emit(second_adapter),
        Err(VmPersistentActorTelemetryError::CardinalityLimitExceeded {
            dimension: "adapter_id",
            limit: 1,
        })
    );

    let failure_limits = VmPersistentActorTelemetryLimits {
        failure_reasons: 0,
        ..limits
    };
    let mut failure_collector =
        VmPersistentActorTelemetryCollector::new("actor-1", "orders", failure_limits)
            .expect("failure collector");
    let mut failure = event(VmPersistentActorTelemetryKind::AdapterFailure);
    failure.typed_failure_reason = Some("adapter_timeout".to_string());
    assert_eq!(
        failure_collector.emit(failure),
        Err(VmPersistentActorTelemetryError::CardinalityLimitExceeded {
            dimension: "failure_reason",
            limit: 0,
        })
    );

    assert_eq!(
        VmPersistentActorTelemetryCollector::new(
            "",
            "orders",
            VmPersistentActorTelemetryLimits::default()
        ),
        Err(VmPersistentActorTelemetryError::MissingActorIdentity { sequence: 0 })
    );
    let empty = VmPersistentActorTelemetryCollector::new(
        "actor-1",
        "orders",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("empty collector");
    assert_eq!(
        empty.finish(),
        Err(VmPersistentActorTelemetryError::EmptyTrace)
    );

    let mut sequence_overflow = VmPersistentActorTelemetryCollector::new(
        "actor-1",
        "orders",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("overflow collector");
    sequence_overflow.next_sequence = u64::MAX;
    assert_eq!(
        sequence_overflow.emit(event(VmPersistentActorTelemetryKind::Append)),
        Err(VmPersistentActorTelemetryError::CounterOverflow {
            sequence: u64::MAX,
            field: "sequence",
        })
    );
}

#[test]
fn vm_persistent_actor_telemetry_rejects_mixed_identity_and_counter_overflow() {
    let mut mixed = deterministic_restore_trace();
    mixed[1].actor_id = "actor-2".to_string();
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&mixed),
        Err(VmPersistentActorTelemetryError::ActorIdentityMismatch { sequence: 2 })
    );

    let mut overflow = deterministic_restore_trace();
    overflow[0].scheduler_ticks = u64::MAX;
    overflow[1].scheduler_ticks = 1;
    assert_eq!(
        validate_persistent_actor_telemetry_trace(&overflow),
        Err(VmPersistentActorTelemetryError::CounterOverflow {
            sequence: 2,
            field: "scheduler_ticks",
        })
    );
}

#[test]
fn vm_persistent_actor_telemetry_lifecycle_emits_store_and_restore_spans() {
    let actor_id = VmPersistentActorId::new("orders-1").expect("actor id");
    let schema = VmPersistentActorSchema::new("Order", 1).expect("schema");
    let mut lifecycle = VmPersistentActorTelemetryLifecycle::new(
        VmInMemoryPersistentActorStore::new(),
        actor_id.clone(),
        "orders",
        "embedded-key-value",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("telemetry lifecycle");
    let snapshot = VmPersistentActorSnapshot::new(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::String("pending".to_string()),
        vec![ReplValue::String("ship".to_string())],
        vec![50],
        vec!["postgres.private-token".to_string()],
        0,
    )
    .expect("snapshot");

    assert!(matches!(
        lifecycle.store_snapshot(snapshot),
        Ok(VmPersistentActorStoreOutcome::SnapshotStored(_))
    ));
    let actor_event = VmPersistentActorEvent::new(
        actor_id,
        schema.clone(),
        1,
        ReplValue::String("shipped".to_string()),
    )
    .expect("event");
    assert!(matches!(
        lifecycle.append_event(actor_event),
        Ok(VmPersistentActorStoreOutcome::EventAppended(_))
    ));
    assert!(lifecycle.replay(&schema).expect("telemetry").is_ok());

    let kinds = lifecycle
        .telemetry_spans()
        .iter()
        .map(|span| span.kind.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            VmPersistentActorTelemetryKind::Snapshot,
            VmPersistentActorTelemetryKind::Checkpoint,
            VmPersistentActorTelemetryKind::Append,
            VmPersistentActorTelemetryKind::Snapshot,
            VmPersistentActorTelemetryKind::Replay,
            VmPersistentActorTelemetryKind::MailboxRestore,
            VmPersistentActorTelemetryKind::TimerRestore,
            VmPersistentActorTelemetryKind::ResourceValidation,
            VmPersistentActorTelemetryKind::Restore,
        ]
    );
    assert!(lifecycle
        .telemetry_spans()
        .iter()
        .filter_map(|span| span.redacted_resource_label.as_deref())
        .all(|label| label == "[redacted-resource]"));

    let report = lifecycle
        .finish_with_metrics(VmPersistentActorMetricLimits::default())
        .expect("valid lifecycle report");
    assert_eq!(report.trace.actor_id, "orders-1");
    assert_eq!(report.trace.total_scheduler_ticks, 9);
    assert_eq!(report.trace.failure_classification, None);
    assert_eq!(report.metrics.len(), 8);
    assert_eq!(
        report
            .metrics
            .iter()
            .map(|series| series.span_count)
            .sum::<u64>(),
        9
    );
}

#[test]
fn vm_persistent_actor_telemetry_lifecycle_rejects_identity_drift_and_traces_failures() {
    let actor_id = VmPersistentActorId::new("orders-1").expect("actor id");
    let other_actor_id = VmPersistentActorId::new("orders-2").expect("actor id");
    let schema = VmPersistentActorSchema::new("Order", 1).expect("schema");
    let mut lifecycle = VmPersistentActorTelemetryLifecycle::new(
        VmInMemoryPersistentActorStore::new(),
        actor_id,
        "orders",
        "local-durable",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("telemetry lifecycle");
    let wrong_actor_event =
        VmPersistentActorEvent::new(other_actor_id.clone(), schema.clone(), 1, ReplValue::Unit)
            .expect("event");

    assert_eq!(
        lifecycle.append_event(wrong_actor_event),
        Err(VmPersistentActorTelemetryLifecycleError::ActorIdentityMismatch)
    );
    assert!(lifecycle.telemetry_spans().is_empty());

    let partial =
        VmPersistentActorEvent::new(other_actor_id, schema, 2, ReplValue::Unit).expect("event");
    let mut failure_lifecycle = VmPersistentActorTelemetryLifecycle::new(
        VmInMemoryPersistentActorStore::new(),
        partial.actor_id.clone(),
        "orders",
        "local-durable",
        VmPersistentActorTelemetryLimits::default(),
    )
    .expect("telemetry lifecycle");
    assert!(matches!(
        failure_lifecycle.reject_partial_event(partial),
        Ok(VmPersistentActorStoreOutcome::PartialWriteRejected { .. })
    ));
    let trace = failure_lifecycle.finish().expect("failure trace");
    assert_eq!(
        trace.failure_classification.as_deref(),
        Some("partial_write_rejected")
    );

    assert_eq!(
        VmPersistentActorTelemetryLifecycle::new(
            VmInMemoryPersistentActorStore::new(),
            VmPersistentActorId::new("orders-3").expect("actor id"),
            "orders",
            "",
            VmPersistentActorTelemetryLimits::default(),
        )
        .err(),
        Some(VmPersistentActorTelemetryLifecycleError::MissingAdapterIdentity)
    );
}
