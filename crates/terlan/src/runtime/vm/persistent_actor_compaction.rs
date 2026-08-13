use std::collections::BTreeSet;

use super::persistent_actor_store::{
    VmPersistentActorEvent, VmPersistentActorReplay, VmPersistentActorSnapshot,
};
use super::ReplValue;

/// Explicit retention rules used before actor history can be physically pruned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorRetentionPolicy {
    pub(crate) retain_from_sequence: u64,
    pub(crate) schema_migration_floor: u64,
    pub(crate) audit_floor: u64,
    pub(crate) allow_mailbox_checkpoint_prune: bool,
    pub(crate) allow_timer_checkpoint_prune: bool,
    pub(crate) allow_resource_handle_cleanup: bool,
}

impl VmPersistentActorRetentionPolicy {
    pub(crate) fn new(retain_from_sequence: u64) -> Self {
        Self {
            retain_from_sequence,
            schema_migration_floor: 0,
            audit_floor: 0,
            allow_mailbox_checkpoint_prune: false,
            allow_timer_checkpoint_prune: false,
            allow_resource_handle_cleanup: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_schema_migration_floor(mut self, floor: u64) -> Self {
        self.schema_migration_floor = floor;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_audit_floor(mut self, floor: u64) -> Self {
        self.audit_floor = floor;
        self
    }
}

/// Replay result supplied by the actor runtime after applying the event log.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorReplayEquivalence {
    pub(crate) final_state: ReplValue,
    pub(crate) final_mailbox_checkpoint: Vec<ReplValue>,
    pub(crate) final_timer_checkpoint: Vec<u64>,
    pub(crate) final_resource_handles: Vec<String>,
    pub(crate) final_sequence: u64,
}

impl VmPersistentActorReplayEquivalence {
    pub(crate) fn from_snapshot(snapshot: &VmPersistentActorSnapshot) -> Self {
        Self {
            final_state: snapshot.state.clone(),
            final_mailbox_checkpoint: snapshot.mailbox_checkpoint.clone(),
            final_timer_checkpoint: snapshot.timer_checkpoint.clone(),
            final_resource_handles: snapshot.resource_handles.clone(),
            final_sequence: snapshot.last_event_sequence,
        }
    }
}

/// Proposed physical compaction result before it is committed to an adapter.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorCompactionCandidate {
    pub(crate) snapshot: VmPersistentActorSnapshot,
    pub(crate) retained_events: Vec<VmPersistentActorEvent>,
}

/// VM-owned safe compaction plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorCompactionPlan {
    pub(crate) compacted_snapshot_generation: u64,
    pub(crate) retained_event_sequences: Vec<u64>,
    pub(crate) retained_resource_handles: Vec<String>,
    pub(crate) reclaimed_resource_handles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmPersistentActorCompactionError {
    EmptyRetentionPolicy,
    RetentionBeforeSchemaMigrationFloor {
        retain_from_sequence: u64,
        schema_migration_floor: u64,
    },
    RetentionBeforeAuditFloor {
        retain_from_sequence: u64,
        audit_floor: u64,
    },
    ActorChanged,
    SchemaChanged,
    SnapshotGenerationNotAdvanced,
    FinalSequenceMovedBackward,
    CompactedSnapshotNotEquivalent,
    RetainedEventBeforeCompactedSnapshot {
        sequence: u64,
        compacted_sequence: u64,
    },
    RetainedEventGap {
        expected: u64,
        actual: u64,
    },
    RetainedEventNotInOriginalLog {
        sequence: u64,
    },
    MailboxCheckpointPrunedWithoutPolicy,
    TimerCheckpointPrunedWithoutPolicy,
    ResourceHandlePrunedWithoutPolicy {
        handle: String,
    },
}

/// Validates that a compacted snapshot plus retained event suffix is safe.
pub(crate) fn plan_persistent_actor_compaction(
    before: &VmPersistentActorReplay,
    replay_equivalence: &VmPersistentActorReplayEquivalence,
    candidate: &VmPersistentActorCompactionCandidate,
    policy: &VmPersistentActorRetentionPolicy,
) -> Result<VmPersistentActorCompactionPlan, VmPersistentActorCompactionError> {
    if policy.retain_from_sequence == 0 {
        return Err(VmPersistentActorCompactionError::EmptyRetentionPolicy);
    }
    if policy.retain_from_sequence < policy.schema_migration_floor {
        return Err(
            VmPersistentActorCompactionError::RetentionBeforeSchemaMigrationFloor {
                retain_from_sequence: policy.retain_from_sequence,
                schema_migration_floor: policy.schema_migration_floor,
            },
        );
    }
    if policy.retain_from_sequence < policy.audit_floor {
        return Err(
            VmPersistentActorCompactionError::RetentionBeforeAuditFloor {
                retain_from_sequence: policy.retain_from_sequence,
                audit_floor: policy.audit_floor,
            },
        );
    }
    if candidate.snapshot.actor_id != before.snapshot.actor_id {
        return Err(VmPersistentActorCompactionError::ActorChanged);
    }
    if candidate.snapshot.schema != before.snapshot.schema {
        return Err(VmPersistentActorCompactionError::SchemaChanged);
    }
    if candidate.snapshot.generation <= before.snapshot.generation {
        return Err(VmPersistentActorCompactionError::SnapshotGenerationNotAdvanced);
    }
    if replay_equivalence.final_sequence < before.snapshot.last_event_sequence {
        return Err(VmPersistentActorCompactionError::FinalSequenceMovedBackward);
    }
    if candidate.snapshot.state != replay_equivalence.final_state
        || candidate.snapshot.mailbox_checkpoint != replay_equivalence.final_mailbox_checkpoint
        || candidate.snapshot.timer_checkpoint != replay_equivalence.final_timer_checkpoint
        || candidate.snapshot.resource_handles != replay_equivalence.final_resource_handles
        || candidate.snapshot.last_event_sequence != replay_equivalence.final_sequence
    {
        return Err(VmPersistentActorCompactionError::CompactedSnapshotNotEquivalent);
    }

    validate_checkpoint_retention(before, &candidate.snapshot, policy)?;
    validate_retained_event_suffix(before, candidate)?;

    let retained_resource_handles = candidate.snapshot.resource_handles.clone();
    let retained_set: BTreeSet<_> = retained_resource_handles.iter().cloned().collect();
    let reclaimed_resource_handles = before
        .snapshot
        .resource_handles
        .iter()
        .filter(|handle| !retained_set.contains(*handle))
        .cloned()
        .collect();

    Ok(VmPersistentActorCompactionPlan {
        compacted_snapshot_generation: candidate.snapshot.generation,
        retained_event_sequences: candidate
            .retained_events
            .iter()
            .map(|event| event.sequence)
            .collect(),
        retained_resource_handles,
        reclaimed_resource_handles,
    })
}

fn validate_checkpoint_retention(
    before: &VmPersistentActorReplay,
    compacted: &VmPersistentActorSnapshot,
    policy: &VmPersistentActorRetentionPolicy,
) -> Result<(), VmPersistentActorCompactionError> {
    if !policy.allow_mailbox_checkpoint_prune
        && compacted.mailbox_checkpoint.len() < before.snapshot.mailbox_checkpoint.len()
    {
        return Err(VmPersistentActorCompactionError::MailboxCheckpointPrunedWithoutPolicy);
    }
    if !policy.allow_timer_checkpoint_prune
        && compacted.timer_checkpoint.len() < before.snapshot.timer_checkpoint.len()
    {
        return Err(VmPersistentActorCompactionError::TimerCheckpointPrunedWithoutPolicy);
    }
    if !policy.allow_resource_handle_cleanup {
        for handle in &before.snapshot.resource_handles {
            if !compacted.resource_handles.contains(handle) {
                return Err(
                    VmPersistentActorCompactionError::ResourceHandlePrunedWithoutPolicy {
                        handle: handle.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

fn validate_retained_event_suffix(
    before: &VmPersistentActorReplay,
    candidate: &VmPersistentActorCompactionCandidate,
) -> Result<(), VmPersistentActorCompactionError> {
    let original_sequences: BTreeSet<u64> =
        before.events.iter().map(|event| event.sequence).collect();
    let compacted_sequence = candidate.snapshot.last_event_sequence;
    let mut expected = Some(compacted_sequence + 1);

    for event in &candidate.retained_events {
        if event.actor_id != candidate.snapshot.actor_id {
            return Err(VmPersistentActorCompactionError::ActorChanged);
        }
        if event.schema != candidate.snapshot.schema {
            return Err(VmPersistentActorCompactionError::SchemaChanged);
        }
        if event.sequence <= compacted_sequence {
            return Err(
                VmPersistentActorCompactionError::RetainedEventBeforeCompactedSnapshot {
                    sequence: event.sequence,
                    compacted_sequence,
                },
            );
        }
        if !original_sequences.contains(&event.sequence) {
            return Err(
                VmPersistentActorCompactionError::RetainedEventNotInOriginalLog {
                    sequence: event.sequence,
                },
            );
        }
        if let Some(expected_sequence) = expected {
            if event.sequence != expected_sequence {
                return Err(VmPersistentActorCompactionError::RetainedEventGap {
                    expected: expected_sequence,
                    actual: event.sequence,
                });
            }
        }
        expected = Some(event.sequence + 1);
    }

    Ok(())
}

#[cfg(test)]
#[path = "persistent_actor_compaction_test.rs"]
#[cfg(test)]
mod persistent_actor_compaction_test;
