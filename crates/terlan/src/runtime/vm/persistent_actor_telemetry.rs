#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use super::model_sync::VmModelSyncChange;
use super::persistent_actor_store::{
    VmPersistentActorEvent, VmPersistentActorId, VmPersistentActorReplay, VmPersistentActorSchema,
    VmPersistentActorSnapshot, VmPersistentActorStoreAdapter, VmPersistentActorStoreOutcome,
};

#[path = "persistent_actor_telemetry/lifecycle_finish.rs"]
mod lifecycle_finish;

#[derive(Clone, Debug, Ord, PartialEq, PartialOrd, Eq)]
pub(crate) enum VmPersistentActorTelemetryKind {
    Append,
    Snapshot,
    Checkpoint,
    Replay,
    SchemaMigration,
    Compaction,
    Export,
    MailboxRestore,
    TimerRestore,
    Restore,
    ResourceValidation,
    PostRecoveryMessage,
    ModelSyncPublication,
    AdapterFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetryEvent {
    pub(crate) kind: VmPersistentActorTelemetryKind,
    pub(crate) schema_id: String,
    pub(crate) snapshot_generation: u64,
    pub(crate) event_start: u64,
    pub(crate) event_end: u64,
    pub(crate) adapter_id: String,
    pub(crate) scheduler_ticks: u64,
    pub(crate) durable_bytes: u64,
    pub(crate) retry_count: u64,
    pub(crate) recovery_phase: String,
    pub(crate) typed_failure_reason: Option<String>,
    pub(crate) resource_label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetryLimits {
    pub(crate) schema_ids: usize,
    pub(crate) adapter_ids: usize,
    pub(crate) failure_reasons: usize,
}

impl Default for VmPersistentActorTelemetryLimits {
    fn default() -> Self {
        Self {
            schema_ids: 16,
            adapter_ids: 8,
            failure_reasons: 16,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetryCollector {
    actor_id: String,
    actor_family: String,
    limits: VmPersistentActorTelemetryLimits,
    next_sequence: u64,
    terminal_failure: Option<String>,
    rollback_completed: bool,
    schema_ids: BTreeSet<String>,
    adapter_ids: BTreeSet<String>,
    failure_reasons: BTreeSet<String>,
    model_sync_sequences: BTreeMap<String, u64>,
    spans: Vec<VmPersistentActorTelemetrySpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetrySpan {
    pub(crate) sequence: u64,
    pub(crate) kind: VmPersistentActorTelemetryKind,
    pub(crate) actor_id: String,
    pub(crate) actor_family: String,
    pub(crate) schema_id: String,
    pub(crate) snapshot_generation: u64,
    pub(crate) event_start: u64,
    pub(crate) event_end: u64,
    pub(crate) adapter_id: String,
    pub(crate) scheduler_ticks: u64,
    pub(crate) durable_bytes: u64,
    pub(crate) retry_count: u64,
    pub(crate) recovery_phase: String,
    pub(crate) typed_failure_reason: Option<String>,
    pub(crate) redacted_resource_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetryTrace {
    pub(crate) actor_id: String,
    pub(crate) replay_timeline: Vec<String>,
    pub(crate) total_scheduler_ticks: u64,
    pub(crate) total_durable_bytes: u64,
    pub(crate) failure_classification: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorDebuggerHandoff {
    pub(crate) source_map_id: String,
    pub(crate) replay_step: u64,
    pub(crate) actor_id: String,
    pub(crate) snapshot_generation: u64,
    pub(crate) operation: VmPersistentActorTelemetryKind,
    pub(crate) event_start: u64,
    pub(crate) event_end: u64,
    pub(crate) typed_failure_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetrySupportPolicy {
    private: (),
}

impl VmPersistentActorTelemetrySupportPolicy {
    pub(crate) fn redacted() -> Self {
        Self { private: () }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetrySupportStep {
    pub(crate) sequence: u64,
    pub(crate) operation: VmPersistentActorTelemetryKind,
    pub(crate) snapshot_generation: u64,
    pub(crate) event_start: u64,
    pub(crate) event_end: u64,
    pub(crate) scheduler_ticks: u64,
    pub(crate) durable_bytes: u64,
    pub(crate) retry_count: u64,
    pub(crate) failed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorTelemetrySupportBundle {
    pub(crate) actor_reference: &'static str,
    pub(crate) steps: Vec<VmPersistentActorTelemetrySupportStep>,
    pub(crate) total_scheduler_ticks: u64,
    pub(crate) total_durable_bytes: u64,
    pub(crate) failed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmPersistentActorTelemetryError {
    EmptyTrace,
    MissingActorIdentity {
        sequence: u64,
    },
    ActorIdentityMismatch {
        sequence: u64,
    },
    DuplicateSequence {
        sequence: u64,
    },
    OutOfOrderSequence {
        previous: u64,
        next: u64,
    },
    EmptyEventRange {
        sequence: u64,
    },
    UnredactedSecret {
        sequence: u64,
    },
    MisleadingSuccessAfterFailure {
        sequence: u64,
    },
    FailureClassificationChanged {
        sequence: u64,
    },
    TelemetryAfterRollback,
    CardinalityLimitExceeded {
        dimension: &'static str,
        limit: usize,
    },
    CounterOverflow {
        sequence: u64,
        field: &'static str,
    },
    MissingSourceMapIdentity,
    ReplayStepUnavailable {
        replay_step: u64,
    },
    EmptyModelSyncStream,
    InvalidModelSyncChange {
        sequence: u64,
    },
    ModelSyncSequenceRegression {
        model: String,
        previous: u64,
        next: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmPersistentActorTelemetryLifecycleError {
    ActorIdentityMismatch,
    MissingAdapterIdentity,
    Telemetry(VmPersistentActorTelemetryError),
}

impl From<VmPersistentActorTelemetryError> for VmPersistentActorTelemetryLifecycleError {
    fn from(error: VmPersistentActorTelemetryError) -> Self {
        Self::Telemetry(error)
    }
}

/// VM-owned persistence orchestration that makes telemetry part of each store operation.
pub(crate) struct VmPersistentActorTelemetryLifecycle<A> {
    adapter: A,
    actor_id: VmPersistentActorId,
    adapter_id: String,
    collector: VmPersistentActorTelemetryCollector,
}

impl<A: VmPersistentActorStoreAdapter> VmPersistentActorTelemetryLifecycle<A> {
    pub(crate) fn new(
        adapter: A,
        actor_id: VmPersistentActorId,
        actor_family: impl Into<String>,
        adapter_id: impl Into<String>,
        limits: VmPersistentActorTelemetryLimits,
    ) -> Result<Self, VmPersistentActorTelemetryLifecycleError> {
        let adapter_id = adapter_id.into();
        if adapter_id.is_empty() {
            return Err(VmPersistentActorTelemetryLifecycleError::MissingAdapterIdentity);
        }
        let collector =
            VmPersistentActorTelemetryCollector::new(actor_id.as_str(), actor_family, limits)?;
        Ok(Self {
            adapter,
            actor_id,
            adapter_id,
            collector,
        })
    }

    pub(crate) fn store_snapshot(
        &mut self,
        snapshot: VmPersistentActorSnapshot,
    ) -> Result<VmPersistentActorStoreOutcome, VmPersistentActorTelemetryLifecycleError> {
        self.ensure_actor(&snapshot.actor_id)?;
        let metadata = SnapshotTelemetryMetadata::from_snapshot(&snapshot);
        let outcome = self.adapter.store_snapshot(snapshot);
        if let Some(reason) = store_failure_reason(&outcome) {
            self.emit(metadata.event(
                VmPersistentActorTelemetryKind::AdapterFailure,
                &self.adapter_id,
                Some(reason),
            ))?;
        } else {
            self.emit(metadata.event(
                VmPersistentActorTelemetryKind::Snapshot,
                &self.adapter_id,
                None,
            ))?;
            self.emit(metadata.event(
                VmPersistentActorTelemetryKind::Checkpoint,
                &self.adapter_id,
                None,
            ))?;
        }
        Ok(outcome)
    }

    pub(crate) fn append_event(
        &mut self,
        event: VmPersistentActorEvent,
    ) -> Result<VmPersistentActorStoreOutcome, VmPersistentActorTelemetryLifecycleError> {
        self.ensure_actor(&event.actor_id)?;
        let telemetry = event_telemetry(
            VmPersistentActorTelemetryKind::Append,
            &event.schema,
            0,
            event.sequence,
            &self.adapter_id,
            None,
            None,
            "append",
        );
        let outcome = self.adapter.append_event(event);
        if let Some(reason) = store_failure_reason(&outcome) {
            self.emit(VmPersistentActorTelemetryEvent {
                kind: VmPersistentActorTelemetryKind::AdapterFailure,
                typed_failure_reason: Some(reason.to_string()),
                ..telemetry
            })?;
        } else {
            self.emit(telemetry)?;
        }
        Ok(outcome)
    }

    pub(crate) fn reject_partial_event(
        &mut self,
        event: VmPersistentActorEvent,
    ) -> Result<VmPersistentActorStoreOutcome, VmPersistentActorTelemetryLifecycleError> {
        self.ensure_actor(&event.actor_id)?;
        let telemetry = event_telemetry(
            VmPersistentActorTelemetryKind::AdapterFailure,
            &event.schema,
            0,
            event.sequence,
            &self.adapter_id,
            Some("partial_write_rejected"),
            None,
            "append",
        );
        let outcome = self.adapter.reject_partial_event(event);
        self.emit(telemetry)?;
        Ok(outcome)
    }

    pub(crate) fn replay(
        &mut self,
        expected_schema: &VmPersistentActorSchema,
    ) -> Result<
        Result<VmPersistentActorReplay, VmPersistentActorStoreOutcome>,
        VmPersistentActorTelemetryLifecycleError,
    > {
        let replay = self.adapter.replay(&self.actor_id, expected_schema);
        match &replay {
            Ok(replayed) => self.emit_replay_lifecycle(replayed)?,
            Err(outcome) => {
                let sequence = outcome_event_sequence(outcome);
                self.emit(event_telemetry(
                    VmPersistentActorTelemetryKind::AdapterFailure,
                    expected_schema,
                    0,
                    sequence,
                    &self.adapter_id,
                    Some(store_failure_reason(outcome).unwrap_or("replay_rejected")),
                    None,
                    "restore",
                ))?;
            }
        }
        Ok(replay)
    }

    pub(crate) fn telemetry_spans(&self) -> &[VmPersistentActorTelemetrySpan] {
        self.collector.spans()
    }

    fn ensure_actor(
        &self,
        actor_id: &VmPersistentActorId,
    ) -> Result<(), VmPersistentActorTelemetryLifecycleError> {
        if actor_id != &self.actor_id {
            return Err(VmPersistentActorTelemetryLifecycleError::ActorIdentityMismatch);
        }
        Ok(())
    }

    fn emit(
        &mut self,
        event: VmPersistentActorTelemetryEvent,
    ) -> Result<(), VmPersistentActorTelemetryLifecycleError> {
        self.collector.emit(event).map_err(Into::into)
    }

    fn emit_replay_lifecycle(
        &mut self,
        replay: &VmPersistentActorReplay,
    ) -> Result<(), VmPersistentActorTelemetryLifecycleError> {
        let metadata = SnapshotTelemetryMetadata::from_snapshot(&replay.snapshot);
        let replay_end = replay
            .events
            .last()
            .map_or(replay.snapshot.last_event_sequence, |event| event.sequence);
        let kinds = [
            VmPersistentActorTelemetryKind::Snapshot,
            VmPersistentActorTelemetryKind::Replay,
            VmPersistentActorTelemetryKind::MailboxRestore,
            VmPersistentActorTelemetryKind::TimerRestore,
            VmPersistentActorTelemetryKind::ResourceValidation,
            VmPersistentActorTelemetryKind::Restore,
        ];
        for kind in kinds {
            let mut event = metadata.event(kind, &self.adapter_id, None);
            event.event_end = replay_end;
            self.emit(event)?;
        }
        Ok(())
    }
}

struct SnapshotTelemetryMetadata {
    schema: VmPersistentActorSchema,
    generation: u64,
    event_sequence: u64,
    resource_label: Option<String>,
}

impl SnapshotTelemetryMetadata {
    fn from_snapshot(snapshot: &VmPersistentActorSnapshot) -> Self {
        Self {
            schema: snapshot.schema.clone(),
            generation: snapshot.generation,
            event_sequence: snapshot.last_event_sequence,
            resource_label: snapshot.resource_handles.first().cloned(),
        }
    }

    fn event(
        &self,
        kind: VmPersistentActorTelemetryKind,
        adapter_id: &str,
        failure_reason: Option<&str>,
    ) -> VmPersistentActorTelemetryEvent {
        event_telemetry(
            kind,
            &self.schema,
            self.generation,
            self.event_sequence,
            adapter_id,
            failure_reason,
            self.resource_label.clone(),
            "restore",
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn event_telemetry(
    kind: VmPersistentActorTelemetryKind,
    schema: &VmPersistentActorSchema,
    snapshot_generation: u64,
    event_sequence: u64,
    adapter_id: &str,
    failure_reason: Option<&str>,
    resource_label: Option<String>,
    recovery_phase: &str,
) -> VmPersistentActorTelemetryEvent {
    VmPersistentActorTelemetryEvent {
        kind,
        schema_id: format!("{}-v{}", schema.id, schema.version),
        snapshot_generation,
        event_start: event_sequence,
        event_end: event_sequence,
        adapter_id: adapter_id.to_string(),
        scheduler_ticks: 1,
        durable_bytes: 0,
        retry_count: 0,
        recovery_phase: recovery_phase.to_string(),
        typed_failure_reason: failure_reason.map(str::to_string),
        resource_label,
    }
}

fn store_failure_reason(outcome: &VmPersistentActorStoreOutcome) -> Option<&'static str> {
    match outcome {
        VmPersistentActorStoreOutcome::SnapshotStored(_)
        | VmPersistentActorStoreOutcome::EventAppended(_)
        | VmPersistentActorStoreOutcome::Replayed(_) => None,
        VmPersistentActorStoreOutcome::MissingSnapshot(_) => Some("missing_snapshot"),
        VmPersistentActorStoreOutcome::StaleSnapshot { .. } => Some("stale_snapshot"),
        VmPersistentActorStoreOutcome::StaleEvent { .. } => Some("stale_event"),
        VmPersistentActorStoreOutcome::DuplicateEvent { .. } => Some("duplicate_event"),
        VmPersistentActorStoreOutcome::IncompatibleSchema { .. } => Some("incompatible_schema"),
        VmPersistentActorStoreOutcome::PartialWriteRejected { .. } => {
            Some("partial_write_rejected")
        }
        VmPersistentActorStoreOutcome::PersistenceFailed { .. } => Some("persistence_failed"),
    }
}

fn outcome_event_sequence(outcome: &VmPersistentActorStoreOutcome) -> u64 {
    match outcome {
        VmPersistentActorStoreOutcome::SnapshotStored(snapshot) => snapshot.last_event_sequence,
        VmPersistentActorStoreOutcome::EventAppended(event)
        | VmPersistentActorStoreOutcome::Replayed(event) => event.sequence,
        VmPersistentActorStoreOutcome::StaleSnapshot {
            incoming_generation,
            ..
        } => *incoming_generation,
        VmPersistentActorStoreOutcome::StaleEvent {
            incoming_sequence, ..
        } => *incoming_sequence,
        VmPersistentActorStoreOutcome::DuplicateEvent { sequence, .. }
        | VmPersistentActorStoreOutcome::PartialWriteRejected { sequence, .. } => *sequence,
        VmPersistentActorStoreOutcome::MissingSnapshot(_)
        | VmPersistentActorStoreOutcome::IncompatibleSchema { .. }
        | VmPersistentActorStoreOutcome::PersistenceFailed { .. } => 0,
    }
}

impl VmPersistentActorTelemetryCollector {
    pub(crate) fn new(
        actor_id: impl Into<String>,
        actor_family: impl Into<String>,
        limits: VmPersistentActorTelemetryLimits,
    ) -> Result<Self, VmPersistentActorTelemetryError> {
        let actor_id = actor_id.into();
        let actor_family = actor_family.into();
        if actor_id.is_empty() || actor_family.is_empty() {
            return Err(VmPersistentActorTelemetryError::MissingActorIdentity { sequence: 0 });
        }
        Ok(Self {
            actor_id,
            actor_family,
            limits,
            next_sequence: 1,
            terminal_failure: None,
            rollback_completed: false,
            schema_ids: BTreeSet::new(),
            adapter_ids: BTreeSet::new(),
            failure_reasons: BTreeSet::new(),
            model_sync_sequences: BTreeMap::new(),
            spans: Vec::new(),
        })
    }

    pub(crate) fn emit(
        &mut self,
        event: VmPersistentActorTelemetryEvent,
    ) -> Result<(), VmPersistentActorTelemetryError> {
        if self.rollback_completed {
            return Err(VmPersistentActorTelemetryError::TelemetryAfterRollback);
        }
        if event.schema_id.is_empty() || event.adapter_id.is_empty() {
            return Err(VmPersistentActorTelemetryError::MissingActorIdentity {
                sequence: self.next_sequence,
            });
        }
        if event.event_end < event.event_start {
            return Err(VmPersistentActorTelemetryError::EmptyEventRange {
                sequence: self.next_sequence,
            });
        }

        ensure_cardinality(
            &self.schema_ids,
            &event.schema_id,
            self.limits.schema_ids,
            "schema_id",
        )?;
        ensure_cardinality(
            &self.adapter_ids,
            &event.adapter_id,
            self.limits.adapter_ids,
            "adapter_id",
        )?;

        let failure_reason = match (&self.terminal_failure, event.typed_failure_reason) {
            (Some(existing), Some(next)) if existing != &next => {
                return Err(
                    VmPersistentActorTelemetryError::FailureClassificationChanged {
                        sequence: self.next_sequence,
                    },
                );
            }
            (Some(existing), _) => Some(existing.clone()),
            (None, Some(reason)) => {
                ensure_cardinality(
                    &self.failure_reasons,
                    &reason,
                    self.limits.failure_reasons,
                    "failure_reason",
                )?;
                self.failure_reasons.insert(reason.clone());
                self.terminal_failure = Some(reason.clone());
                Some(reason)
            }
            (None, None) => None,
        };

        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1).ok_or(
            VmPersistentActorTelemetryError::CounterOverflow {
                sequence,
                field: "sequence",
            },
        )?;
        self.schema_ids.insert(event.schema_id.clone());
        self.adapter_ids.insert(event.adapter_id.clone());
        self.spans.push(VmPersistentActorTelemetrySpan {
            sequence,
            kind: event.kind,
            actor_id: self.actor_id.clone(),
            actor_family: self.actor_family.clone(),
            schema_id: event.schema_id,
            snapshot_generation: event.snapshot_generation,
            event_start: event.event_start,
            event_end: event.event_end,
            adapter_id: event.adapter_id,
            scheduler_ticks: event.scheduler_ticks,
            durable_bytes: event.durable_bytes,
            retry_count: event.retry_count,
            recovery_phase: event.recovery_phase,
            typed_failure_reason: failure_reason,
            redacted_resource_label: event
                .resource_label
                .map(|_| "[redacted-resource]".to_string()),
        });
        Ok(())
    }

    pub(crate) fn complete_rollback(&mut self) {
        self.rollback_completed = true;
    }

    pub(crate) fn spans(&self) -> &[VmPersistentActorTelemetrySpan] {
        &self.spans
    }

    pub(crate) fn publish_model_sync_changes(
        &mut self,
        schema_id: impl Into<String>,
        snapshot_generation: u64,
        adapter_id: impl Into<String>,
        changes: &[VmModelSyncChange],
    ) -> Result<(), VmPersistentActorTelemetryError> {
        if changes.is_empty() {
            return Err(VmPersistentActorTelemetryError::EmptyModelSyncStream);
        }

        let schema_id = schema_id.into();
        let adapter_id = adapter_id.into();
        let mut next_sequences = self.model_sync_sequences.clone();
        for change in changes {
            if change.sequence == 0
                || change.version.sequence == 0
                || change.key.model.is_empty()
                || change.key.id.is_empty()
                || change.version.writer_id.is_empty()
            {
                return Err(VmPersistentActorTelemetryError::InvalidModelSyncChange {
                    sequence: change.sequence,
                });
            }
            if !next_sequences.contains_key(&change.key.model)
                && next_sequences.len() >= self.limits.schema_ids
            {
                return Err(VmPersistentActorTelemetryError::CardinalityLimitExceeded {
                    dimension: "model_sync_model",
                    limit: self.limits.schema_ids,
                });
            }
            if let Some(previous) = next_sequences.get(&change.key.model) {
                if change.sequence <= *previous {
                    return Err(
                        VmPersistentActorTelemetryError::ModelSyncSequenceRegression {
                            model: change.key.model.clone(),
                            previous: *previous,
                            next: change.sequence,
                        },
                    );
                }
            }
            next_sequences.insert(change.key.model.clone(), change.sequence);
        }

        let event_count = u64::try_from(changes.len()).map_err(|_| {
            VmPersistentActorTelemetryError::CounterOverflow {
                sequence: self.next_sequence,
                field: "sequence",
            }
        })?;
        self.next_sequence.checked_add(event_count).ok_or(
            VmPersistentActorTelemetryError::CounterOverflow {
                sequence: self.next_sequence,
                field: "sequence",
            },
        )?;
        ensure_cardinality(
            &self.schema_ids,
            &schema_id,
            self.limits.schema_ids,
            "schema_id",
        )?;
        ensure_cardinality(
            &self.adapter_ids,
            &adapter_id,
            self.limits.adapter_ids,
            "adapter_id",
        )?;

        for change in changes {
            self.emit(VmPersistentActorTelemetryEvent {
                kind: VmPersistentActorTelemetryKind::ModelSyncPublication,
                schema_id: schema_id.clone(),
                snapshot_generation,
                event_start: change.sequence,
                event_end: change.sequence,
                adapter_id: adapter_id.clone(),
                scheduler_ticks: 1,
                durable_bytes: 0,
                retry_count: 0,
                recovery_phase: "model_sync".to_string(),
                typed_failure_reason: None,
                resource_label: None,
            })?;
        }
        self.model_sync_sequences = next_sequences;
        Ok(())
    }

    pub(crate) fn finish(
        self,
    ) -> Result<VmPersistentActorTelemetryTrace, VmPersistentActorTelemetryError> {
        validate_persistent_actor_telemetry_trace(&self.spans)
    }
}

fn ensure_cardinality(
    values: &BTreeSet<String>,
    candidate: &str,
    limit: usize,
    dimension: &'static str,
) -> Result<(), VmPersistentActorTelemetryError> {
    if !values.contains(candidate) && values.len() >= limit {
        return Err(VmPersistentActorTelemetryError::CardinalityLimitExceeded { dimension, limit });
    }
    Ok(())
}

pub(crate) fn persistent_actor_debugger_handoff(
    spans: &[VmPersistentActorTelemetrySpan],
    source_map_id: impl Into<String>,
    replay_step: u64,
) -> Result<VmPersistentActorDebuggerHandoff, VmPersistentActorTelemetryError> {
    let source_map_id = source_map_id.into();
    if source_map_id.trim().is_empty() {
        return Err(VmPersistentActorTelemetryError::MissingSourceMapIdentity);
    }

    let trace = validate_persistent_actor_telemetry_trace(spans)?;
    let span = spans
        .iter()
        .find(|span| span.sequence == replay_step)
        .ok_or(VmPersistentActorTelemetryError::ReplayStepUnavailable { replay_step })?;

    Ok(VmPersistentActorDebuggerHandoff {
        source_map_id,
        replay_step,
        actor_id: trace.actor_id,
        snapshot_generation: span.snapshot_generation,
        operation: span.kind.clone(),
        event_start: span.event_start,
        event_end: span.event_end,
        typed_failure_reason: span.typed_failure_reason.clone(),
    })
}

pub(crate) fn persistent_actor_telemetry_support_bundle(
    spans: &[VmPersistentActorTelemetrySpan],
    _policy: &VmPersistentActorTelemetrySupportPolicy,
) -> Result<VmPersistentActorTelemetrySupportBundle, VmPersistentActorTelemetryError> {
    let trace = validate_persistent_actor_telemetry_trace(spans)?;
    let failed = trace.failure_classification.is_some();
    let steps = spans
        .iter()
        .map(|span| VmPersistentActorTelemetrySupportStep {
            sequence: span.sequence,
            operation: span.kind.clone(),
            snapshot_generation: span.snapshot_generation,
            event_start: span.event_start,
            event_end: span.event_end,
            scheduler_ticks: span.scheduler_ticks,
            durable_bytes: span.durable_bytes,
            retry_count: span.retry_count,
            failed: span.typed_failure_reason.is_some(),
        })
        .collect();

    Ok(VmPersistentActorTelemetrySupportBundle {
        actor_reference: "[redacted-actor]",
        steps,
        total_scheduler_ticks: trace.total_scheduler_ticks,
        total_durable_bytes: trace.total_durable_bytes,
        failed,
    })
}

pub(crate) fn validate_persistent_actor_telemetry_trace(
    spans: &[VmPersistentActorTelemetrySpan],
) -> Result<VmPersistentActorTelemetryTrace, VmPersistentActorTelemetryError> {
    if spans.is_empty() {
        return Err(VmPersistentActorTelemetryError::EmptyTrace);
    }

    let mut previous_sequence = None;
    let mut failed = false;
    let mut failure_classification = None;
    let mut replay_timeline = Vec::with_capacity(spans.len());
    let mut total_scheduler_ticks: u64 = 0;
    let mut total_durable_bytes: u64 = 0;
    let actor_id = spans[0].actor_id.clone();
    let actor_family = spans[0].actor_family.clone();

    for span in spans {
        if span.actor_id.is_empty() || span.actor_family.is_empty() || span.schema_id.is_empty() {
            return Err(VmPersistentActorTelemetryError::MissingActorIdentity {
                sequence: span.sequence,
            });
        }
        if span.actor_id != actor_id || span.actor_family != actor_family {
            return Err(VmPersistentActorTelemetryError::ActorIdentityMismatch {
                sequence: span.sequence,
            });
        }
        if span.event_end < span.event_start {
            return Err(VmPersistentActorTelemetryError::EmptyEventRange {
                sequence: span.sequence,
            });
        }
        if let Some(previous) = previous_sequence {
            if span.sequence == previous {
                return Err(VmPersistentActorTelemetryError::DuplicateSequence {
                    sequence: span.sequence,
                });
            }
            if span.sequence < previous {
                return Err(VmPersistentActorTelemetryError::OutOfOrderSequence {
                    previous,
                    next: span.sequence,
                });
            }
        }
        if span
            .redacted_resource_label
            .as_deref()
            .is_some_and(contains_secret_material)
        {
            return Err(VmPersistentActorTelemetryError::UnredactedSecret {
                sequence: span.sequence,
            });
        }
        if failed && span.typed_failure_reason.is_none() {
            return Err(
                VmPersistentActorTelemetryError::MisleadingSuccessAfterFailure {
                    sequence: span.sequence,
                },
            );
        }

        if let Some(reason) = &span.typed_failure_reason {
            failed = true;
            failure_classification.get_or_insert_with(|| reason.clone());
        }

        previous_sequence = Some(span.sequence);
        total_scheduler_ticks = total_scheduler_ticks
            .checked_add(span.scheduler_ticks)
            .ok_or(VmPersistentActorTelemetryError::CounterOverflow {
                sequence: span.sequence,
                field: "scheduler_ticks",
            })?;
        total_durable_bytes = total_durable_bytes.checked_add(span.durable_bytes).ok_or(
            VmPersistentActorTelemetryError::CounterOverflow {
                sequence: span.sequence,
                field: "durable_bytes",
            },
        )?;
        replay_timeline.push(format!(
            "{}:{}",
            span.sequence,
            telemetry_kind_name(&span.kind)
        ));
    }

    Ok(VmPersistentActorTelemetryTrace {
        actor_id,
        replay_timeline,
        total_scheduler_ticks,
        total_durable_bytes,
        failure_classification,
    })
}

pub(crate) fn deterministic_restore_trace() -> Vec<VmPersistentActorTelemetrySpan> {
    vec![
        span(1, VmPersistentActorTelemetryKind::Snapshot, 0, 8),
        span(2, VmPersistentActorTelemetryKind::Replay, 9, 16),
        span(3, VmPersistentActorTelemetryKind::MailboxRestore, 16, 16),
        span(4, VmPersistentActorTelemetryKind::TimerRestore, 16, 16),
        span(
            5,
            VmPersistentActorTelemetryKind::ResourceValidation,
            16,
            16,
        ),
        span(
            6,
            VmPersistentActorTelemetryKind::PostRecoveryMessage,
            16,
            16,
        ),
    ]
}

fn span(
    sequence: u64,
    kind: VmPersistentActorTelemetryKind,
    event_start: u64,
    event_end: u64,
) -> VmPersistentActorTelemetrySpan {
    VmPersistentActorTelemetrySpan {
        sequence,
        kind,
        actor_id: "actor-1".to_string(),
        actor_family: "orders".to_string(),
        schema_id: "orders-v1".to_string(),
        snapshot_generation: 7,
        event_start,
        event_end,
        adapter_id: "local-durable".to_string(),
        scheduler_ticks: sequence * 3,
        durable_bytes: sequence * 128,
        retry_count: 0,
        recovery_phase: "restore".to_string(),
        typed_failure_reason: None,
        redacted_resource_label: Some("[redacted-resource]".to_string()),
    }
}

fn telemetry_kind_name(kind: &VmPersistentActorTelemetryKind) -> &'static str {
    match kind {
        VmPersistentActorTelemetryKind::Append => "append",
        VmPersistentActorTelemetryKind::Snapshot => "snapshot",
        VmPersistentActorTelemetryKind::Checkpoint => "checkpoint",
        VmPersistentActorTelemetryKind::Replay => "replay",
        VmPersistentActorTelemetryKind::SchemaMigration => "schema_migration",
        VmPersistentActorTelemetryKind::Compaction => "compaction",
        VmPersistentActorTelemetryKind::Export => "export",
        VmPersistentActorTelemetryKind::MailboxRestore => "mailbox_restore",
        VmPersistentActorTelemetryKind::TimerRestore => "timer_restore",
        VmPersistentActorTelemetryKind::Restore => "restore",
        VmPersistentActorTelemetryKind::ResourceValidation => "resource_validation",
        VmPersistentActorTelemetryKind::PostRecoveryMessage => "post_recovery_message",
        VmPersistentActorTelemetryKind::ModelSyncPublication => "model_sync_publication",
        VmPersistentActorTelemetryKind::AdapterFailure => "adapter_failure",
    }
}

fn contains_secret_material(label: &str) -> bool {
    let normalized = label.to_ascii_lowercase();
    normalized.contains("secret")
        || normalized.contains("token")
        || normalized.contains("password")
        || normalized.contains("private")
}

#[cfg(test)]
#[path = "persistent_actor_telemetry_test.rs"]
mod persistent_actor_telemetry_test;
