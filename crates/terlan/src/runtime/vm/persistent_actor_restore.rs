use std::collections::{BTreeMap, BTreeSet};

use super::model_sync::VmModelSyncChange;
use super::persistent_actor_store::{
    VmPersistentActorEvent, VmPersistentActorId, VmPersistentActorSchema, VmPersistentActorSnapshot,
};
#[cfg(any(test, feature = "benchmark-tools"))]
use super::persistent_actor_store::{VmPersistentActorStoreAdapter, VmPersistentActorStoreOutcome};
use super::ReplValue;

const DEFAULT_RESTORE_ADAPTER_KIND: &str = "force_local";
const CROSS_MACHINE_EXPORT_FORMAT: &str = "terlan-vm-persistent-actor-export-v1";

/// Deterministic persistent actor export produced before physical restore.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorExport {
    pub(crate) snapshot: VmPersistentActorSnapshot,
    pub(crate) retained_events: Vec<VmPersistentActorEvent>,
    pub(crate) model_sync_changes: Vec<VmModelSyncChange>,
    pub(crate) redacted_fields: Vec<String>,
    pub(crate) compacted: bool,
    pub(crate) source_adapter_kind: String,
    pub(crate) checksum: String,
}

impl VmPersistentActorExport {
    pub(crate) fn new(
        snapshot: VmPersistentActorSnapshot,
        retained_events: Vec<VmPersistentActorEvent>,
        redacted_fields: Vec<String>,
        compacted: bool,
    ) -> Result<Self, VmPersistentActorRestoreError> {
        let export = Self {
            snapshot,
            retained_events,
            model_sync_changes: Vec::new(),
            redacted_fields,
            compacted,
            source_adapter_kind: DEFAULT_RESTORE_ADAPTER_KIND.to_string(),
            checksum: String::new(),
        };
        validate_mailbox_checkpoint_order(&export.snapshot)?;
        validate_export_suffix(&export.snapshot, &export.retained_events)?;
        Ok(Self {
            checksum: export.compute_checksum(),
            ..export
        })
    }

    #[cfg(test)]
    pub(crate) fn with_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.checksum = checksum.into();
        self
    }

    pub(crate) fn with_source_adapter_kind(
        mut self,
        source_adapter_kind: impl Into<String>,
    ) -> Self {
        self.source_adapter_kind = source_adapter_kind.into();
        self.checksum = self.compute_checksum();
        self
    }

    #[cfg(test)]
    pub(crate) fn with_model_sync_changes(
        mut self,
        model_sync_changes: Vec<VmModelSyncChange>,
    ) -> Self {
        self.model_sync_changes = model_sync_changes;
        self.checksum = self.compute_checksum();
        self
    }

    fn compute_checksum(&self) -> String {
        let event_sequences = self
            .retained_events
            .iter()
            .map(|event| event.sequence.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let model_sync_sequences = self
            .model_sync_changes
            .iter()
            .map(|change| format!("{}:{}", change.key.model, change.sequence))
            .collect::<Vec<_>>()
            .join(",");
        let resources = self.snapshot.resource_handles.join(",");
        let redactions = self.redacted_fields.join(",");
        format!(
            "actor={:?};schema={}:{};generation={};sequence={};events={};model_sync={};resources={};redactions={};compacted={};adapter={}",
            self.snapshot.actor_id,
            self.snapshot.schema.id,
            self.snapshot.schema.version,
            self.snapshot.generation,
            self.snapshot.last_event_sequence,
            event_sequences,
            model_sync_sequences,
            resources,
            redactions,
            self.compacted,
            self.source_adapter_kind,
        )
    }
}

/// Capabilities declared by the destination adapter before restore.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorRestoreCapabilities {
    pub(crate) supports_compacted_snapshot_restore: bool,
    pub(crate) supports_resource_handle_restore: bool,
}

impl VmPersistentActorRestoreCapabilities {
    pub(crate) fn full() -> Self {
        Self {
            supports_compacted_snapshot_restore: true,
            supports_resource_handle_restore: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn without_compaction() -> Self {
        Self {
            supports_compacted_snapshot_restore: false,
            supports_resource_handle_restore: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn without_resource_handles() -> Self {
        Self {
            supports_compacted_snapshot_restore: true,
            supports_resource_handle_restore: false,
        }
    }
}

/// Restore destination constraints supplied by the VM, not by the adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorRestoreTarget {
    pub(crate) actor_id: VmPersistentActorId,
    pub(crate) schema: VmPersistentActorSchema,
    pub(crate) available_resource_handles: BTreeSet<String>,
    pub(crate) capabilities: VmPersistentActorRestoreCapabilities,
    pub(crate) adapter_kind: String,
    pub(crate) allow_cross_adapter_restore: bool,
    pub(crate) required_model_sync_streams: Vec<VmPersistentActorModelSyncContinuity>,
}

impl VmPersistentActorRestoreTarget {
    pub(crate) fn new(
        actor_id: VmPersistentActorId,
        schema: VmPersistentActorSchema,
        available_resource_handles: impl IntoIterator<Item = String>,
        capabilities: VmPersistentActorRestoreCapabilities,
    ) -> Self {
        Self {
            actor_id,
            schema,
            available_resource_handles: available_resource_handles.into_iter().collect(),
            capabilities,
            adapter_kind: DEFAULT_RESTORE_ADAPTER_KIND.to_string(),
            allow_cross_adapter_restore: false,
            required_model_sync_streams: Vec::new(),
        }
    }

    pub(crate) fn with_adapter_kind(mut self, adapter_kind: impl Into<String>) -> Self {
        self.adapter_kind = adapter_kind.into();
        self
    }

    pub(crate) fn allow_cross_adapter_restore(mut self) -> Self {
        self.allow_cross_adapter_restore = true;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_required_model_sync_streams(
        mut self,
        required_model_sync_streams: Vec<VmPersistentActorModelSyncContinuity>,
    ) -> Self {
        self.required_model_sync_streams = required_model_sync_streams;
        self
    }
}

/// Model stream window that must be present before actor restore is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorModelSyncContinuity {
    pub(crate) model: String,
    pub(crate) retained_from_sequence: u64,
}

impl VmPersistentActorModelSyncContinuity {
    #[cfg(test)]
    pub(crate) fn new(model: impl Into<String>, retained_from_sequence: u64) -> Self {
        Self {
            model: model.into(),
            retained_from_sequence,
        }
    }
}

/// VM-owned restore plan accepted by an adapter only after validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorRestorePlan {
    pub(crate) snapshot_generation: u64,
    pub(crate) restored_event_sequences: Vec<u64>,
    pub(crate) restored_resource_handles: Vec<String>,
    pub(crate) redacted_fields: Vec<String>,
    pub(crate) compaction_restore: Option<VmPersistentActorCompactionRestore>,
    pub(crate) model_sync_streams: Vec<VmPersistentActorModelSyncRestoreStream>,
}

/// Restore metadata required when an export starts from a compacted snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorCompactionRestore {
    pub(crate) compacted_snapshot_generation: u64,
    pub(crate) compacted_through_sequence: u64,
    pub(crate) retained_suffix_start: Option<u64>,
    pub(crate) retained_suffix_end: Option<u64>,
}

/// Accepted model-sync stream window restored alongside an actor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorModelSyncRestoreStream {
    pub(crate) model: String,
    pub(crate) retained_from_sequence: u64,
    pub(crate) retained_to_sequence: u64,
    pub(crate) change_count: usize,
}

/// Actor-visible outcome after a validated export has been written to a store.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) struct VmPersistentActorRestoreExecution {
    pub(crate) source_adapter_kind: String,
    pub(crate) destination_adapter_kind: String,
    pub(crate) snapshot_generation: u64,
    pub(crate) restored_event_count: usize,
    pub(crate) replayed_event_count: usize,
}

/// Deterministic, adapter-independent export envelope for machine transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorCrossMachineExport {
    pub(crate) format_version: &'static str,
    pub(crate) source_machine_id: String,
    pub(crate) actor_id: String,
    pub(crate) schema_id: String,
    pub(crate) schema_version: u64,
    pub(crate) snapshot_generation: u64,
    pub(crate) snapshot_last_event_sequence: u64,
    pub(crate) retained_event_sequences: Vec<u64>,
    pub(crate) resource_handle_count: usize,
    pub(crate) redacted_fields: Vec<String>,
    pub(crate) model_sync_streams: Vec<String>,
    pub(crate) export_checksum: String,
}

impl VmPersistentActorCrossMachineExport {
    pub(crate) fn render_manifest(&self) -> String {
        format!(
            "format={};source_machine={};actor={};schema={}:{};generation={};last_sequence={};events={};resource_handle_count={};redactions={};model_sync_streams={};checksum={}",
            self.format_version,
            self.source_machine_id,
            self.actor_id,
            self.schema_id,
            self.schema_version,
            self.snapshot_generation,
            self.snapshot_last_event_sequence,
            self.retained_event_sequences
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            self.resource_handle_count,
            self.redacted_fields.join(","),
            self.model_sync_streams.join(","),
            self.export_checksum,
        )
    }
}

/// Deterministic, metadata-only replay fixture for failed actor restores.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorReplayFixture {
    pub(crate) actor_id: String,
    pub(crate) schema_id: String,
    pub(crate) schema_version: u64,
    pub(crate) snapshot_generation: u64,
    pub(crate) snapshot_last_event_sequence: u64,
    pub(crate) retained_event_sequences: Vec<u64>,
    pub(crate) mailbox_checkpoint_count: usize,
    pub(crate) mailbox_checkpoint_sequences: Vec<u64>,
    pub(crate) timer_deadlines: Vec<u64>,
    pub(crate) resource_handles: Vec<String>,
    pub(crate) redacted_fields: Vec<String>,
    pub(crate) compacted: bool,
    pub(crate) compacted_through_sequence: Option<u64>,
    pub(crate) retained_suffix_start: Option<u64>,
    pub(crate) model_sync_streams: Vec<String>,
    pub(crate) export_checksum: String,
}

impl VmPersistentActorReplayFixture {
    pub(crate) fn render_manifest(&self) -> String {
        format!(
            "actor={};schema={}:{};generation={};last_sequence={};events={};mailbox_count={};mailbox_sequences={};timers={};resources={};redactions={};compacted={};compacted_through={};retained_suffix_start={};model_sync_streams={};checksum={}",
            self.actor_id,
            self.schema_id,
            self.schema_version,
            self.snapshot_generation,
            self.snapshot_last_event_sequence,
            self.retained_event_sequences
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            self.mailbox_checkpoint_count,
            self.mailbox_checkpoint_sequences
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            self.timer_deadlines
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            self.resource_handles.join(","),
            self.redacted_fields.join(","),
            self.compacted,
            optional_u64(self.compacted_through_sequence),
            optional_u64(self.retained_suffix_start),
            self.model_sync_streams.join(","),
            self.export_checksum,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmPersistentActorRestoreError {
    CorruptExportChecksum,
    WrongActorOwner,
    StaleSchema,
    IncompatibleAdapterForCompactedSnapshot,
    IncompatibleAdapterForResourceHandles,
    IncompatibleAdapterKind {
        expected: String,
        actual: String,
    },
    MissingDurableResourceHandle {
        handle: String,
    },
    MissingModelSyncContinuity {
        model: String,
    },
    ReorderedModelSyncStream {
        expected: u64,
        actual: u64,
    },
    #[cfg(any(test, feature = "benchmark-tools"))]
    StoreRejected {
        step: &'static str,
        outcome: &'static str,
    },
    InvalidCrossMachineExportSource {
        source_machine_id: String,
    },
    EventActorChanged {
        sequence: u64,
    },
    EventSchemaChanged {
        sequence: u64,
    },
    ReorderedRetainedEventSuffix {
        expected: u64,
        actual: u64,
    },
    ReorderedMailboxCheckpoint {
        expected: u64,
        actual: u64,
    },
}

/// Validates a persistent actor export before any adapter can restore it.
pub(crate) fn plan_persistent_actor_restore(
    export: &VmPersistentActorExport,
    target: &VmPersistentActorRestoreTarget,
) -> Result<VmPersistentActorRestorePlan, VmPersistentActorRestoreError> {
    if export.checksum != export.compute_checksum() {
        return Err(VmPersistentActorRestoreError::CorruptExportChecksum);
    }
    if export.snapshot.actor_id != target.actor_id {
        return Err(VmPersistentActorRestoreError::WrongActorOwner);
    }
    if export.snapshot.schema != target.schema {
        return Err(VmPersistentActorRestoreError::StaleSchema);
    }
    if export.source_adapter_kind != target.adapter_kind && !target.allow_cross_adapter_restore {
        return Err(VmPersistentActorRestoreError::IncompatibleAdapterKind {
            expected: export.source_adapter_kind.clone(),
            actual: target.adapter_kind.clone(),
        });
    }
    if export.compacted && !target.capabilities.supports_compacted_snapshot_restore {
        return Err(VmPersistentActorRestoreError::IncompatibleAdapterForCompactedSnapshot);
    }
    if !export.snapshot.resource_handles.is_empty()
        && !target.capabilities.supports_resource_handle_restore
    {
        return Err(VmPersistentActorRestoreError::IncompatibleAdapterForResourceHandles);
    }
    for handle in &export.snapshot.resource_handles {
        if !target.available_resource_handles.contains(handle) {
            return Err(
                VmPersistentActorRestoreError::MissingDurableResourceHandle {
                    handle: handle.clone(),
                },
            );
        }
    }
    validate_mailbox_checkpoint_order(&export.snapshot)?;
    validate_export_suffix(&export.snapshot, &export.retained_events)?;
    let model_sync_streams = validate_model_sync_continuity(export, target)?;

    Ok(VmPersistentActorRestorePlan {
        snapshot_generation: export.snapshot.generation,
        restored_event_sequences: export
            .retained_events
            .iter()
            .map(|event| event.sequence)
            .collect(),
        restored_resource_handles: export.snapshot.resource_handles.clone(),
        redacted_fields: export.redacted_fields.clone(),
        compaction_restore: compaction_restore_metadata(export),
        model_sync_streams,
    })
}

/// Builds the portable export envelope used to move an actor between machines.
pub(crate) fn build_cross_machine_actor_export(
    export: &VmPersistentActorExport,
    source_machine_id: impl Into<String>,
) -> Result<VmPersistentActorCrossMachineExport, VmPersistentActorRestoreError> {
    let source_machine_id = source_machine_id.into();
    if !is_valid_cross_machine_source(&source_machine_id) {
        return Err(
            VmPersistentActorRestoreError::InvalidCrossMachineExportSource { source_machine_id },
        );
    }
    if export.checksum != export.compute_checksum() {
        return Err(VmPersistentActorRestoreError::CorruptExportChecksum);
    }
    validate_mailbox_checkpoint_order(&export.snapshot)?;
    validate_export_suffix(&export.snapshot, &export.retained_events)?;

    Ok(VmPersistentActorCrossMachineExport {
        format_version: CROSS_MACHINE_EXPORT_FORMAT,
        source_machine_id,
        actor_id: export.snapshot.actor_id.as_str().to_string(),
        schema_id: export.snapshot.schema.id.clone(),
        schema_version: export.snapshot.schema.version,
        snapshot_generation: export.snapshot.generation,
        snapshot_last_event_sequence: export.snapshot.last_event_sequence,
        retained_event_sequences: export
            .retained_events
            .iter()
            .map(|event| event.sequence)
            .collect(),
        resource_handle_count: export.snapshot.resource_handles.len(),
        redacted_fields: export.redacted_fields.clone(),
        model_sync_streams: export_model_sync_streams(export),
        export_checksum: export.checksum.clone(),
    })
}

/// Validates and executes restore through the destination actor-store adapter.
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) fn execute_persistent_actor_restore(
    export: &VmPersistentActorExport,
    target: &VmPersistentActorRestoreTarget,
    destination: &mut impl VmPersistentActorStoreAdapter,
) -> Result<VmPersistentActorRestoreExecution, VmPersistentActorRestoreError> {
    let plan = plan_persistent_actor_restore(export, target)?;

    let snapshot_outcome = destination.store_snapshot(export.snapshot.clone());
    if !matches!(
        snapshot_outcome,
        VmPersistentActorStoreOutcome::SnapshotStored(_)
    ) {
        return Err(VmPersistentActorRestoreError::StoreRejected {
            step: "store_snapshot",
            outcome: persistent_actor_store_outcome_kind(&snapshot_outcome),
        });
    }

    for event in &export.retained_events {
        let event_outcome = destination.append_event(event.clone());
        if !matches!(
            event_outcome,
            VmPersistentActorStoreOutcome::EventAppended(_)
                | VmPersistentActorStoreOutcome::Replayed(_)
        ) {
            return Err(VmPersistentActorRestoreError::StoreRejected {
                step: "append_event",
                outcome: persistent_actor_store_outcome_kind(&event_outcome),
            });
        }
    }

    let replay = destination
        .replay(&target.actor_id, &target.schema)
        .map_err(|outcome| VmPersistentActorRestoreError::StoreRejected {
            step: "replay",
            outcome: persistent_actor_store_outcome_kind(&outcome),
        })?;

    Ok(VmPersistentActorRestoreExecution {
        source_adapter_kind: export.source_adapter_kind.clone(),
        destination_adapter_kind: target.adapter_kind.clone(),
        snapshot_generation: plan.snapshot_generation,
        restored_event_count: plan.restored_event_sequences.len(),
        replayed_event_count: replay.events.len(),
    })
}

/// Builds a minimal, deterministic replay artifact after restore validation.
pub(crate) fn generate_minimal_actor_replay_fixture(
    export: &VmPersistentActorExport,
    target: &VmPersistentActorRestoreTarget,
) -> Result<VmPersistentActorReplayFixture, VmPersistentActorRestoreError> {
    let plan = plan_persistent_actor_restore(export, target)?;
    let compaction_restore = plan.compaction_restore.clone();
    let model_sync_streams = plan
        .model_sync_streams
        .iter()
        .map(render_model_sync_stream)
        .collect();
    Ok(VmPersistentActorReplayFixture {
        actor_id: export.snapshot.actor_id.as_str().to_string(),
        schema_id: export.snapshot.schema.id.clone(),
        schema_version: export.snapshot.schema.version,
        snapshot_generation: plan.snapshot_generation,
        snapshot_last_event_sequence: export.snapshot.last_event_sequence,
        retained_event_sequences: plan.restored_event_sequences,
        mailbox_checkpoint_count: export.snapshot.mailbox_checkpoint.len(),
        mailbox_checkpoint_sequences: mailbox_checkpoint_sequences(&export.snapshot),
        timer_deadlines: export.snapshot.timer_checkpoint.clone(),
        resource_handles: plan.restored_resource_handles,
        redacted_fields: plan.redacted_fields,
        compacted: export.compacted,
        compacted_through_sequence: compaction_restore
            .as_ref()
            .map(|metadata| metadata.compacted_through_sequence),
        retained_suffix_start: compaction_restore
            .as_ref()
            .and_then(|metadata| metadata.retained_suffix_start),
        model_sync_streams,
        export_checksum: export.checksum.clone(),
    })
}

fn validate_model_sync_continuity(
    export: &VmPersistentActorExport,
    target: &VmPersistentActorRestoreTarget,
) -> Result<Vec<VmPersistentActorModelSyncRestoreStream>, VmPersistentActorRestoreError> {
    let mut streams = Vec::new();
    for required in &target.required_model_sync_streams {
        let mut matched = 0usize;
        let mut last_sequence = None;
        for (expected, change) in (required.retained_from_sequence..).zip(
            export
                .model_sync_changes
                .iter()
                .filter(|change| change.key.model == required.model),
        ) {
            if change.sequence != expected {
                return Err(VmPersistentActorRestoreError::ReorderedModelSyncStream {
                    expected,
                    actual: change.sequence,
                });
            }
            matched += 1;
            last_sequence = Some(change.sequence);
        }
        let Some(retained_to_sequence) = last_sequence else {
            return Err(VmPersistentActorRestoreError::MissingModelSyncContinuity {
                model: required.model.clone(),
            });
        };
        streams.push(VmPersistentActorModelSyncRestoreStream {
            model: required.model.clone(),
            retained_from_sequence: required.retained_from_sequence,
            retained_to_sequence,
            change_count: matched,
        });
    }
    Ok(streams)
}

fn render_model_sync_stream(stream: &VmPersistentActorModelSyncRestoreStream) -> String {
    format!(
        "{}:{}-{}#{}",
        stream.model,
        stream.retained_from_sequence,
        stream.retained_to_sequence,
        stream.change_count
    )
}

fn export_model_sync_streams(export: &VmPersistentActorExport) -> Vec<String> {
    let mut streams: BTreeMap<&str, (u64, u64, usize)> = BTreeMap::new();
    for change in &export.model_sync_changes {
        streams
            .entry(change.key.model.as_str())
            .and_modify(|(first, last, count)| {
                *first = (*first).min(change.sequence);
                *last = (*last).max(change.sequence);
                *count += 1;
            })
            .or_insert((change.sequence, change.sequence, 1));
    }
    streams
        .into_iter()
        .map(|(model, (first, last, count))| {
            render_model_sync_stream(&VmPersistentActorModelSyncRestoreStream {
                model: model.to_string(),
                retained_from_sequence: first,
                retained_to_sequence: last,
                change_count: count,
            })
        })
        .collect()
}

fn is_valid_cross_machine_source(source_machine_id: &str) -> bool {
    !source_machine_id.is_empty()
        && source_machine_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

#[cfg(any(test, feature = "benchmark-tools"))]
fn persistent_actor_store_outcome_kind(outcome: &VmPersistentActorStoreOutcome) -> &'static str {
    match outcome {
        VmPersistentActorStoreOutcome::SnapshotStored(_) => "snapshot_stored",
        VmPersistentActorStoreOutcome::EventAppended(_) => "event_appended",
        VmPersistentActorStoreOutcome::Replayed(_) => "replayed",
        VmPersistentActorStoreOutcome::MissingSnapshot(_) => "missing_snapshot",
        VmPersistentActorStoreOutcome::StaleSnapshot { .. } => "stale_snapshot",
        VmPersistentActorStoreOutcome::StaleEvent { .. } => "stale_event",
        VmPersistentActorStoreOutcome::DuplicateEvent { .. } => "duplicate_event",
        VmPersistentActorStoreOutcome::IncompatibleSchema { .. } => "incompatible_schema",
        #[cfg(test)]
        VmPersistentActorStoreOutcome::PartialWriteRejected { .. } => "partial_write_rejected",
        VmPersistentActorStoreOutcome::PersistenceFailed { .. } => "persistence_failed",
    }
}

fn compaction_restore_metadata(
    export: &VmPersistentActorExport,
) -> Option<VmPersistentActorCompactionRestore> {
    export
        .compacted
        .then(|| VmPersistentActorCompactionRestore {
            compacted_snapshot_generation: export.snapshot.generation,
            compacted_through_sequence: export.snapshot.last_event_sequence,
            retained_suffix_start: export.retained_events.first().map(|event| event.sequence),
            retained_suffix_end: export.retained_events.last().map(|event| event.sequence),
        })
}

fn validate_mailbox_checkpoint_order(
    snapshot: &VmPersistentActorSnapshot,
) -> Result<(), VmPersistentActorRestoreError> {
    let mut expected_sequence = 1;
    for checkpoint in &snapshot.mailbox_checkpoint {
        let Some(actual_sequence) = mailbox_checkpoint_sequence(checkpoint) else {
            continue;
        };
        if actual_sequence != expected_sequence {
            return Err(VmPersistentActorRestoreError::ReorderedMailboxCheckpoint {
                expected: expected_sequence,
                actual: actual_sequence,
            });
        }
        expected_sequence += 1;
    }
    Ok(())
}

fn mailbox_checkpoint_sequences(snapshot: &VmPersistentActorSnapshot) -> Vec<u64> {
    snapshot
        .mailbox_checkpoint
        .iter()
        .filter_map(mailbox_checkpoint_sequence)
        .collect()
}

fn mailbox_checkpoint_sequence(value: &ReplValue) -> Option<u64> {
    match value {
        ReplValue::Tuple(items) => match items.as_slice() {
            [ReplValue::Atom(tag), ReplValue::Int(sequence), ..]
                if tag == "mailbox_checkpoint" && *sequence >= 0 =>
            {
                Some(*sequence as u64)
            }
            _ => None,
        },
        _ => None,
    }
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn validate_export_suffix(
    snapshot: &VmPersistentActorSnapshot,
    retained_events: &[VmPersistentActorEvent],
) -> Result<(), VmPersistentActorRestoreError> {
    for (expected_sequence, event) in
        (snapshot.last_event_sequence + 1..).zip(retained_events.iter())
    {
        if event.actor_id != snapshot.actor_id {
            return Err(VmPersistentActorRestoreError::EventActorChanged {
                sequence: event.sequence,
            });
        }
        if event.schema != snapshot.schema {
            return Err(VmPersistentActorRestoreError::EventSchemaChanged {
                sequence: event.sequence,
            });
        }
        if event.sequence != expected_sequence {
            return Err(
                VmPersistentActorRestoreError::ReorderedRetainedEventSuffix {
                    expected: expected_sequence,
                    actual: event.sequence,
                },
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "persistent_actor_restore_test.rs"]
#[cfg(test)]
mod persistent_actor_restore_test;
