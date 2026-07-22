//! Tests for execution-shard epoch fencing and recovery admission.

use super::*;

/// Creates a validated epoch.
fn epoch(value: u64) -> VmShardEpoch {
    VmShardEpoch::new(value).expect("shard epoch")
}

/// Creates a validated operation identity.
fn operation_id(value: u64) -> VmShardOperationId {
    VmShardOperationId::new(value).expect("operation identity")
}

/// Creates one explicitly classified epoch-bound operation.
fn operation(
    id: u64,
    epoch_value: u64,
    kind: VmShardOperationKind,
    replay_policy: VmShardReplayPolicy,
) -> VmShardEpochOperation {
    VmShardEpochOperation::new(operation_id(id), epoch(epoch_value), kind, replay_policy)
}

/// Every runtime ingress and effect class rejects a stale generation.
#[test]
fn every_epoch_bound_operation_class_rejects_stale_generation() {
    assert_eq!(VmShardOperationKind::ALL.len(), 9);
    for (index, kind) in VmShardOperationKind::ALL.into_iter().enumerate() {
        let mut fence = VmShardEpochFence::new(epoch(1));
        fence.advance(epoch(2)).expect("advance epoch");
        let stale = operation(
            u64::try_from(index).unwrap() + 1,
            1,
            kind,
            VmShardReplayPolicy::AtMostOnce,
        );
        assert_eq!(
            fence.begin(stale),
            Err(VmShardEpochError::StaleEpoch {
                expected: epoch(2),
                actual: epoch(1),
            })
        );
        assert_eq!(fence.operation_count(), 0);
    }
}

/// A committed operation remains suppressed after a shard restart.
#[test]
fn committed_external_effect_cannot_execute_twice_across_epochs() {
    let mut fence = VmShardEpochFence::new(epoch(1));
    let first = operation(
        7,
        1,
        VmShardOperationKind::HttpResponse,
        VmShardReplayPolicy::AtMostOnce,
    );
    assert_eq!(
        fence.begin(first),
        Ok(VmShardOperationAdmission::ExecuteFirst)
    );
    assert_eq!(
        fence.begin(first),
        Ok(VmShardOperationAdmission::DuplicateSuppressed)
    );
    assert_eq!(fence.commit(first), Ok(VmShardOperationCommit::Committed));
    assert_eq!(
        fence.commit(first),
        Ok(VmShardOperationCommit::AlreadyCommitted)
    );

    fence.advance(epoch(2)).expect("advance epoch");
    let replay = VmShardEpochOperation {
        epoch: epoch(2),
        ..first
    };
    assert_eq!(
        fence.begin(replay),
        Ok(VmShardOperationAdmission::DuplicateSuppressed)
    );
    assert_eq!(fence.operation_count(), 1);
}

/// Recovery policy decides whether an uncertain operation can run again.
#[test]
fn interrupted_operations_require_explicit_recovery_policy() {
    let mut fence = VmShardEpochFence::new(epoch(1));
    let replayable = operation(
        11,
        1,
        VmShardOperationKind::ActorRoute,
        VmShardReplayPolicy::Replayable,
    );
    let idempotent = operation(
        12,
        1,
        VmShardOperationKind::DatabaseWrite,
        VmShardReplayPolicy::Idempotent,
    );
    let at_most_once = operation(
        13,
        1,
        VmShardOperationKind::MailboxPublication,
        VmShardReplayPolicy::AtMostOnce,
    );
    for candidate in [replayable, idempotent, at_most_once] {
        assert_eq!(
            fence.begin(candidate),
            Ok(VmShardOperationAdmission::ExecuteFirst)
        );
    }

    fence.advance(epoch(2)).expect("advance epoch");
    let replayable = VmShardEpochOperation {
        epoch: epoch(2),
        ..replayable
    };
    let idempotent = VmShardEpochOperation {
        epoch: epoch(2),
        ..idempotent
    };
    let at_most_once = VmShardEpochOperation {
        epoch: epoch(2),
        ..at_most_once
    };
    assert_eq!(
        fence.begin(replayable),
        Ok(VmShardOperationAdmission::ExecuteReplay)
    );
    assert_eq!(
        fence.begin(idempotent),
        Ok(VmShardOperationAdmission::ExecuteIdempotentReplay)
    );
    assert_eq!(
        fence.begin(at_most_once),
        Ok(VmShardOperationAdmission::IndeterminateSuppressed)
    );
    assert_eq!(
        fence.commit(at_most_once),
        Err(VmShardEpochError::OperationNotPreparedForEpoch {
            operation_id: operation_id(13),
            prepared: epoch(1),
            completion: epoch(2),
        })
    );
    assert_eq!(
        fence.commit(replayable),
        Ok(VmShardOperationCommit::Committed)
    );
    assert_eq!(
        fence.commit(idempotent),
        Ok(VmShardOperationCommit::Committed)
    );
}

/// Stable operation ids cannot be rebound to another kind or replay policy.
#[test]
fn operation_identity_conflicts_and_unknown_completions_fail_closed() {
    assert_eq!(
        VmShardOperationId::new(0),
        Err(VmShardEpochError::ZeroOperationId)
    );
    assert_eq!(operation_id(41).as_u64(), 41);
    let mut fence = VmShardEpochFence::new(epoch(4));
    assert_eq!(fence.epoch(), epoch(4));
    let original = operation(
        41,
        4,
        VmShardOperationKind::TimerDelivery,
        VmShardReplayPolicy::AtMostOnce,
    );
    fence.begin(original).expect("admit original operation");
    let conflicting_kind = VmShardEpochOperation {
        kind: VmShardOperationKind::ExternalEffect,
        ..original
    };
    assert_eq!(
        fence.begin(conflicting_kind),
        Err(VmShardEpochError::OperationIdentityConflict {
            operation_id: operation_id(41),
        })
    );
    let conflicting_policy = VmShardEpochOperation {
        replay_policy: VmShardReplayPolicy::Idempotent,
        ..original
    };
    assert_eq!(
        fence.commit(conflicting_policy),
        Err(VmShardEpochError::OperationIdentityConflict {
            operation_id: operation_id(41),
        })
    );
    let unknown = operation(
        42,
        4,
        VmShardOperationKind::CapabilityCompletion,
        VmShardReplayPolicy::AtMostOnce,
    );
    assert_eq!(
        fence.commit(unknown),
        Err(VmShardEpochError::UnknownOperation {
            operation_id: operation_id(42),
        })
    );
    assert_eq!(fence.operation_count(), 1);
}

/// Epoch replacement must strictly advance and preserve the prior state on error.
#[test]
fn epoch_fence_rejects_reuse_and_regression_without_mutation() {
    let mut fence = VmShardEpochFence::new(epoch(8));
    let original = fence.clone();
    assert_eq!(
        fence.advance(epoch(8)),
        Err(VmShardEpochError::NonAdvancingEpoch {
            current: epoch(8),
            proposed: epoch(8),
        })
    );
    assert_eq!(fence, original);
    assert_eq!(
        fence.advance(epoch(7)),
        Err(VmShardEpochError::NonAdvancingEpoch {
            current: epoch(8),
            proposed: epoch(7),
        })
    );
    assert_eq!(fence, original);
}
