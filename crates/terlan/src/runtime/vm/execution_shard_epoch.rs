//! Epoch fencing and recovery-safe operation admission for execution shards.

use std::collections::BTreeMap;

use super::execution_shard_protocol::VmShardEpoch;

/// Stable identity of one operation across shard restarts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmShardOperationId(
    /// Validated nonzero operation number.
    u64,
);

impl VmShardOperationId {
    /// Creates a nonzero operation identity.
    pub(crate) const fn new(value: u64) -> Result<Self, VmShardEpochError> {
        if value == 0 {
            return Err(VmShardEpochError::ZeroOperationId);
        }
        Ok(Self(value))
    }

    /// Returns the numeric operation identity.
    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Runtime ingress or effect class protected by a shard epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardOperationKind {
    /// Actor traffic entering from another shard.
    ActorRoute,
    /// Publication of a message into an actor mailbox.
    MailboxPublication,
    /// Resume of one parked native continuation.
    ContinuationResume,
    /// Notification emitted by a runtime resource.
    ResourceNotification,
    /// Completion returned by an asynchronous capability worker.
    CapabilityCompletion,
    /// Delivery of one timer effect.
    TimerDelivery,
    /// Publication of one HTTP response.
    HttpResponse,
    /// Dispatch of one database write.
    DatabaseWrite,
    /// Another externally observable effect.
    ExternalEffect,
}

impl VmShardOperationKind {
    /// Every ingress and external-effect class covered by epoch fencing.
    pub(crate) const ALL: [Self; 9] = [
        Self::ActorRoute,
        Self::MailboxPublication,
        Self::ContinuationResume,
        Self::ResourceNotification,
        Self::CapabilityCompletion,
        Self::TimerDelivery,
        Self::HttpResponse,
        Self::DatabaseWrite,
        Self::ExternalEffect,
    ];
}

/// Explicit recovery behavior for one operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardReplayPolicy {
    /// Re-execution is safe because the operation has no non-replayable effect.
    Replayable,
    /// Re-execution is safe only with the stable operation id as an idempotency key.
    Idempotent,
    /// An uncertain dispatch must never execute again automatically.
    AtMostOnce,
}

impl VmShardReplayPolicy {
    /// Every explicit recovery policy admitted by the operation ledger.
    pub(crate) const ALL: [Self; 3] = [Self::Replayable, Self::Idempotent, Self::AtMostOnce];
}

/// One epoch-bound operation presented to the shard runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmShardEpochOperation {
    /// Stable identity retained across retries and restarts.
    pub(crate) id: VmShardOperationId,
    /// Exact shard generation that submitted this attempt.
    pub(crate) epoch: VmShardEpoch,
    /// Runtime ingress or effect class.
    pub(crate) kind: VmShardOperationKind,
    /// Explicit crash-recovery behavior.
    pub(crate) replay_policy: VmShardReplayPolicy,
}

impl VmShardEpochOperation {
    /// Creates an operation with no implicit replay default.
    pub(crate) const fn new(
        id: VmShardOperationId,
        epoch: VmShardEpoch,
        kind: VmShardOperationKind,
        replay_policy: VmShardReplayPolicy,
    ) -> Self {
        Self {
            id,
            epoch,
            kind,
            replay_policy,
        }
    }
}

/// Admission decision made before an operation may execute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardOperationAdmission {
    /// This is the first admitted attempt and may execute.
    ExecuteFirst,
    /// A prior replayable attempt was interrupted and may execute again.
    ExecuteReplay,
    /// A prior attempt may execute with its stable external idempotency key.
    ExecuteIdempotentReplay,
    /// The operation already committed or is already in flight.
    DuplicateSuppressed,
    /// An at-most-once attempt crossed a crash with an uncertain outcome.
    IndeterminateSuppressed,
}

/// Completion decision after one admitted operation finishes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardOperationCommit {
    /// The prepared operation became durably committed in the VM ledger.
    Committed,
    /// The operation had already committed and no state changed.
    AlreadyCommitted,
}

/// Typed rejection from shard epoch and replay enforcement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmShardEpochError {
    /// An operation used the reserved zero identity.
    ZeroOperationId,
    /// An operation named a generation other than the active shard generation.
    StaleEpoch {
        /// Current shard generation.
        expected: VmShardEpoch,
        /// Generation supplied by the operation.
        actual: VmShardEpoch,
    },
    /// A replacement generation did not strictly advance.
    NonAdvancingEpoch {
        /// Current shard generation.
        current: VmShardEpoch,
        /// Rejected replacement generation.
        proposed: VmShardEpoch,
    },
    /// One stable operation identity was reused with different semantics.
    OperationIdentityConflict {
        /// Conflicting operation identity.
        operation_id: VmShardOperationId,
    },
    /// Completion named an operation that was never admitted.
    UnknownOperation {
        /// Missing operation identity.
        operation_id: VmShardOperationId,
    },
    /// Completion did not match the generation of the admitted attempt.
    OperationNotPreparedForEpoch {
        /// Operation whose preparation belongs to another generation.
        operation_id: VmShardOperationId,
        /// Generation that owns the prepared attempt.
        prepared: VmShardEpoch,
        /// Generation that supplied the completion.
        completion: VmShardEpoch,
    },
}

/// Internal execution state retained across shard generations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VmShardOperationState {
    /// Dispatch was admitted but no completion was committed.
    Prepared {
        /// Epoch of the most recent admitted attempt.
        epoch: VmShardEpoch,
    },
    /// The operation completed and must never execute again.
    Committed,
}

/// Internal replay record for one stable operation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VmShardOperationRecord {
    /// Immutable operation class.
    kind: VmShardOperationKind,
    /// Immutable recovery policy.
    replay_policy: VmShardReplayPolicy,
    /// Current execution state.
    state: VmShardOperationState,
}

/// Shard-local generation fence and cross-restart operation ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmShardEpochFence {
    /// Generation currently admitted by the shard supervisor.
    epoch: VmShardEpoch,
    /// Operation identities retained to prevent duplicate external effects.
    operations: BTreeMap<VmShardOperationId, VmShardOperationRecord>,
}

impl VmShardEpochFence {
    /// Creates an empty operation ledger for one admitted generation.
    pub(crate) fn new(epoch: VmShardEpoch) -> Self {
        Self {
            epoch,
            operations: BTreeMap::new(),
        }
    }

    /// Returns the exact active generation.
    #[cfg(test)]
    pub(crate) const fn epoch(&self) -> VmShardEpoch {
        self.epoch
    }

    /// Advances the fence while retaining duplicate-effect evidence.
    pub(crate) fn advance(&mut self, proposed: VmShardEpoch) -> Result<(), VmShardEpochError> {
        if proposed <= self.epoch {
            return Err(VmShardEpochError::NonAdvancingEpoch {
                current: self.epoch,
                proposed,
            });
        }
        self.epoch = proposed;
        Ok(())
    }

    /// Admits one exact-epoch operation according to its recovery policy.
    pub(crate) fn begin(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<VmShardOperationAdmission, VmShardEpochError> {
        debug_assert!(VmShardOperationKind::ALL.contains(&operation.kind));
        debug_assert!(VmShardReplayPolicy::ALL.contains(&operation.replay_policy));
        self.require_epoch(operation.epoch)?;
        let Some(existing) = self.operations.get_mut(&operation.id) else {
            self.operations.insert(
                operation.id,
                VmShardOperationRecord {
                    kind: operation.kind,
                    replay_policy: operation.replay_policy,
                    state: VmShardOperationState::Prepared {
                        epoch: operation.epoch,
                    },
                },
            );
            return Ok(VmShardOperationAdmission::ExecuteFirst);
        };
        if existing.kind != operation.kind || existing.replay_policy != operation.replay_policy {
            return Err(VmShardEpochError::OperationIdentityConflict {
                operation_id: operation.id,
            });
        }
        match existing.state {
            VmShardOperationState::Committed => Ok(VmShardOperationAdmission::DuplicateSuppressed),
            VmShardOperationState::Prepared { epoch } if epoch == operation.epoch => {
                Ok(VmShardOperationAdmission::DuplicateSuppressed)
            }
            VmShardOperationState::Prepared { .. } => match operation.replay_policy {
                VmShardReplayPolicy::Replayable => {
                    existing.state = VmShardOperationState::Prepared {
                        epoch: operation.epoch,
                    };
                    Ok(VmShardOperationAdmission::ExecuteReplay)
                }
                VmShardReplayPolicy::Idempotent => {
                    existing.state = VmShardOperationState::Prepared {
                        epoch: operation.epoch,
                    };
                    Ok(VmShardOperationAdmission::ExecuteIdempotentReplay)
                }
                VmShardReplayPolicy::AtMostOnce => {
                    Ok(VmShardOperationAdmission::IndeterminateSuppressed)
                }
            },
        }
    }

    /// Commits one exact operation after its effect completes successfully.
    pub(crate) fn commit(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<VmShardOperationCommit, VmShardEpochError> {
        self.require_epoch(operation.epoch)?;
        let record =
            self.operations
                .get_mut(&operation.id)
                .ok_or(VmShardEpochError::UnknownOperation {
                    operation_id: operation.id,
                })?;
        if record.kind != operation.kind || record.replay_policy != operation.replay_policy {
            return Err(VmShardEpochError::OperationIdentityConflict {
                operation_id: operation.id,
            });
        }
        match record.state {
            VmShardOperationState::Committed => Ok(VmShardOperationCommit::AlreadyCommitted),
            VmShardOperationState::Prepared { epoch } if epoch == operation.epoch => {
                record.state = VmShardOperationState::Committed;
                Ok(VmShardOperationCommit::Committed)
            }
            VmShardOperationState::Prepared { epoch } => {
                Err(VmShardEpochError::OperationNotPreparedForEpoch {
                    operation_id: operation.id,
                    prepared: epoch,
                    completion: operation.epoch,
                })
            }
        }
    }

    /// Retires a completed in-process operation whose identity cannot be
    /// resubmitted by an external caller.
    pub(crate) fn retire_committed(&mut self, operation: VmShardEpochOperation) -> bool {
        let matches_committed = self.operations.get(&operation.id).is_some_and(|record| {
            record.kind == operation.kind
                && record.replay_policy == operation.replay_policy
                && record.state == VmShardOperationState::Committed
        });
        if matches_committed {
            self.operations.remove(&operation.id);
        }
        matches_committed
    }

    /// Returns the number of stable operation identities retained for recovery.
    #[cfg(test)]
    pub(crate) fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Requires the operation to name the exact active generation.
    fn require_epoch(&self, actual: VmShardEpoch) -> Result<(), VmShardEpochError> {
        if actual != self.epoch {
            return Err(VmShardEpochError::StaleEpoch {
                expected: self.epoch,
                actual,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "execution_shard_epoch_test.rs"]
mod execution_shard_epoch_test;
