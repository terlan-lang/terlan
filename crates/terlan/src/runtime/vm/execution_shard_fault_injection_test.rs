//! Deterministic crash-boundary tests for supervised execution shards.

use super::execution_shard_test_support::{
    image, make_ready, protocol, restart_when_due, supervisor_with_budget,
};
use super::{VmExecutionShardSupervisor, VmShardPhase, VmShardSupervisorError};
use crate::runtime::vm::execution_shard_epoch::{
    VmShardEpochError, VmShardEpochOperation, VmShardOperationAdmission, VmShardOperationCommit,
    VmShardOperationId, VmShardOperationKind, VmShardReplayPolicy,
};
use crate::runtime::vm::execution_shard_protocol::VmShardEpoch;

/// Every required before/after crash boundary in the AOT lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CrashBoundary {
    BeforeAdmission,
    AfterAdmission,
    BeforeReadiness,
    AfterReadiness,
    BeforeMailboxPublication,
    AfterMailboxPublication,
    BeforeContinuationParking,
    AfterContinuationParking,
    BeforeCapabilitySubmission,
    AfterCapabilitySubmission,
    BeforeCapabilityCompletion,
    AfterCapabilityCompletion,
    BeforeDrain,
    AfterDrain,
    BeforeImageReplacement,
    AfterImageReplacement,
}

impl CrashBoundary {
    /// Complete closed crash-injection matrix required by the roadmap.
    const ALL: [Self; 16] = [
        Self::BeforeAdmission,
        Self::AfterAdmission,
        Self::BeforeReadiness,
        Self::AfterReadiness,
        Self::BeforeMailboxPublication,
        Self::AfterMailboxPublication,
        Self::BeforeContinuationParking,
        Self::AfterContinuationParking,
        Self::BeforeCapabilitySubmission,
        Self::AfterCapabilitySubmission,
        Self::BeforeCapabilityCompletion,
        Self::AfterCapabilityCompletion,
        Self::BeforeDrain,
        Self::AfterDrain,
        Self::BeforeImageReplacement,
        Self::AfterImageReplacement,
    ];

    /// Stable diagnostic label recorded as crash ownership evidence.
    const fn label(self) -> &'static str {
        match self {
            Self::BeforeAdmission => "before_admission",
            Self::AfterAdmission => "after_admission",
            Self::BeforeReadiness => "before_readiness",
            Self::AfterReadiness => "after_readiness",
            Self::BeforeMailboxPublication => "before_mailbox_publication",
            Self::AfterMailboxPublication => "after_mailbox_publication",
            Self::BeforeContinuationParking => "before_continuation_parking",
            Self::AfterContinuationParking => "after_continuation_parking",
            Self::BeforeCapabilitySubmission => "before_capability_submission",
            Self::AfterCapabilitySubmission => "after_capability_submission",
            Self::BeforeCapabilityCompletion => "before_capability_completion",
            Self::AfterCapabilityCompletion => "after_capability_completion",
            Self::BeforeDrain => "before_drain",
            Self::AfterDrain => "after_drain",
            Self::BeforeImageReplacement => "before_image_replacement",
            Self::AfterImageReplacement => "after_image_replacement",
        }
    }

    /// Returns whether this boundary belongs to coarse shard lifecycle state.
    const fn is_lifecycle(self) -> bool {
        matches!(
            self,
            Self::BeforeAdmission
                | Self::AfterAdmission
                | Self::BeforeReadiness
                | Self::AfterReadiness
                | Self::BeforeDrain
                | Self::AfterDrain
                | Self::BeforeImageReplacement
                | Self::AfterImageReplacement
        )
    }
}

/// Operation-ledger state present when a crash is injected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InjectedOperationState {
    NotStarted,
    Prepared,
    Committed,
}

/// Builds the epoch operation represented by one actor-runtime crash boundary.
fn operation_for(
    boundary: CrashBoundary,
    id: u64,
    epoch: VmShardEpoch,
) -> (VmShardEpochOperation, InjectedOperationState) {
    let operation_id = VmShardOperationId::new(id).expect("crash operation id");
    let (kind, state) = match boundary {
        CrashBoundary::BeforeMailboxPublication => (
            VmShardOperationKind::MailboxPublication,
            InjectedOperationState::NotStarted,
        ),
        CrashBoundary::AfterMailboxPublication => (
            VmShardOperationKind::MailboxPublication,
            InjectedOperationState::Committed,
        ),
        CrashBoundary::BeforeContinuationParking => (
            VmShardOperationKind::ContinuationResume,
            InjectedOperationState::NotStarted,
        ),
        CrashBoundary::AfterContinuationParking => (
            VmShardOperationKind::ContinuationResume,
            InjectedOperationState::Prepared,
        ),
        CrashBoundary::BeforeCapabilitySubmission => (
            VmShardOperationKind::CapabilityCompletion,
            InjectedOperationState::NotStarted,
        ),
        CrashBoundary::AfterCapabilitySubmission | CrashBoundary::BeforeCapabilityCompletion => (
            VmShardOperationKind::CapabilityCompletion,
            InjectedOperationState::Prepared,
        ),
        CrashBoundary::AfterCapabilityCompletion => (
            VmShardOperationKind::CapabilityCompletion,
            InjectedOperationState::Committed,
        ),
        lifecycle => panic!("lifecycle boundary has no epoch operation: {lifecycle:?}"),
    };
    (
        VmShardEpochOperation::new(operation_id, epoch, kind, VmShardReplayPolicy::AtMostOnce),
        state,
    )
}

/// Advances a supervisor to the exact coarse lifecycle crash boundary.
fn stage_lifecycle_boundary(supervisor: &mut VmExecutionShardSupervisor, boundary: CrashBoundary) {
    match boundary {
        CrashBoundary::BeforeAdmission => {
            supervisor.begin_negotiation().expect("begin negotiation");
            supervisor.negotiate(protocol(1)).expect("negotiate");
        }
        CrashBoundary::AfterAdmission | CrashBoundary::BeforeReadiness => {
            supervisor.begin_negotiation().expect("begin negotiation");
            supervisor.negotiate(protocol(1)).expect("negotiate");
            supervisor
                .admit_image(image("application-v1", 1))
                .expect("admit first image");
        }
        CrashBoundary::AfterReadiness | CrashBoundary::BeforeDrain => {
            make_ready(supervisor, "application-v1", 1);
        }
        CrashBoundary::AfterDrain => {
            let epoch = make_ready(supervisor, "application-v1", 1);
            supervisor.begin_drain(epoch).expect("begin drain");
        }
        CrashBoundary::BeforeImageReplacement | CrashBoundary::AfterImageReplacement => {
            let epoch = make_ready(supervisor, "application-v1", 1);
            supervisor
                .begin_drain(epoch)
                .expect("drain image before replacement");
            if boundary == CrashBoundary::AfterImageReplacement {
                supervisor
                    .replace_drained_image(epoch, image("application-v2", 2))
                    .expect("admit replacement image");
            }
        }
        operation => panic!("operation boundary has no lifecycle stage: {operation:?}"),
    }
}

/// Restarts and admits one replacement while proving old epochs stay fenced.
fn recover_after_injected_crash(
    supervisor: &mut VmExecutionShardSupervisor,
    stale_epoch: Option<VmShardEpoch>,
    digest: u8,
) -> VmShardEpoch {
    restart_when_due(supervisor);
    assert_eq!(supervisor.phase(), VmShardPhase::Negotiating);
    assert_eq!(supervisor.epoch(), None);
    assert_eq!(supervisor.image(), None);
    supervisor.negotiate(protocol(1)).expect("recover protocol");
    let recovered = supervisor
        .admit_image(image("recovered", digest))
        .expect("recover image");
    if let Some(stale) = stale_epoch {
        assert_eq!(
            supervisor.acknowledge_ready(stale),
            Err(VmShardSupervisorError::EpochMismatch {
                expected: recovered,
                actual: stale,
            })
        );
    }
    supervisor
        .acknowledge_ready(recovered)
        .expect("recover readiness");
    assert!(supervisor.is_routable());
    recovered
}

/// Every coarse lifecycle boundary revokes routing and advances recovery epoch.
#[test]
fn lifecycle_crash_boundaries_recover_without_stale_image_resurrection() {
    let lifecycle = CrashBoundary::ALL
        .into_iter()
        .filter(|boundary| boundary.is_lifecycle())
        .collect::<Vec<_>>();
    assert_eq!(lifecycle.len(), 8);
    for (index, boundary) in lifecycle.into_iter().enumerate() {
        let mut supervisor = supervisor_with_budget(4);
        stage_lifecycle_boundary(&mut supervisor, boundary);
        let stale_epoch = supervisor.epoch();
        let reason = format!("injected:{}", boundary.label());
        supervisor
            .report_crash(reason.clone(), 100 + index as u64)
            .expect("inject lifecycle crash");
        assert_eq!(supervisor.phase(), VmShardPhase::RestartBackoff);
        assert!(!supervisor.is_routable());
        let crash = supervisor.last_crash().expect("observable crash report");
        assert_eq!(&crash.shard_id, supervisor.shard_id());
        assert_eq!(crash.epoch, stale_epoch);
        assert_eq!(crash.reason, reason);
        assert_eq!(crash.observed_tick, 100 + index as u64);

        let recovered = recover_after_injected_crash(
            &mut supervisor,
            stale_epoch,
            u8::try_from(index + 10).expect("digest"),
        );
        if let Some(stale) = stale_epoch {
            assert!(recovered > stale);
        }
    }
}

/// Actor-runtime effects have deterministic recovery outcomes at every boundary.
#[test]
fn operation_crash_boundaries_suppress_stale_and_duplicate_completion() {
    let operation_boundaries = CrashBoundary::ALL
        .into_iter()
        .filter(|boundary| !boundary.is_lifecycle())
        .collect::<Vec<_>>();
    assert_eq!(operation_boundaries.len(), 8);
    for (index, boundary) in operation_boundaries.into_iter().enumerate() {
        let mut supervisor = supervisor_with_budget(2);
        let stale_epoch = make_ready(&mut supervisor, "application-v1", 1);
        let (operation, injected_state) = operation_for(boundary, index as u64 + 1, stale_epoch);
        if injected_state != InjectedOperationState::NotStarted {
            assert_eq!(
                supervisor.begin_epoch_operation(operation),
                Ok(VmShardOperationAdmission::ExecuteFirst)
            );
        }
        if injected_state == InjectedOperationState::Committed {
            assert_eq!(
                supervisor.commit_epoch_operation(operation),
                Ok(VmShardOperationCommit::Committed)
            );
        }
        supervisor
            .report_crash(format!("injected:{}", boundary.label()), 200 + index as u64)
            .expect("inject operation crash");
        let recovered_epoch = recover_after_injected_crash(
            &mut supervisor,
            Some(stale_epoch),
            u8::try_from(index + 30).expect("digest"),
        );
        assert_eq!(
            supervisor.begin_epoch_operation(operation),
            Err(VmShardSupervisorError::EpochOperation(
                VmShardEpochError::StaleEpoch {
                    expected: recovered_epoch,
                    actual: stale_epoch,
                }
            ))
        );
        let recovered = VmShardEpochOperation {
            epoch: recovered_epoch,
            ..operation
        };
        let admission = supervisor
            .begin_epoch_operation(recovered)
            .expect("recover operation");
        match injected_state {
            InjectedOperationState::NotStarted => {
                assert_eq!(admission, VmShardOperationAdmission::ExecuteFirst);
                assert_eq!(
                    supervisor.commit_epoch_operation(recovered),
                    Ok(VmShardOperationCommit::Committed)
                );
            }
            InjectedOperationState::Prepared => {
                assert_eq!(
                    admission,
                    VmShardOperationAdmission::IndeterminateSuppressed
                );
                assert!(matches!(
                    supervisor.commit_epoch_operation(recovered),
                    Err(VmShardSupervisorError::EpochOperation(
                        VmShardEpochError::OperationNotPreparedForEpoch { .. }
                    ))
                ));
            }
            InjectedOperationState::Committed => {
                assert_eq!(admission, VmShardOperationAdmission::DuplicateSuppressed);
                assert_eq!(
                    supervisor.commit_epoch_operation(recovered),
                    Ok(VmShardOperationCommit::AlreadyCommitted)
                );
            }
        }
    }
}

/// Restart budget exhaustion produces one immutable, attributed terminal state.
#[test]
fn repeated_injected_crashes_quarantine_with_bounded_restart_and_owned_failure() {
    let mut supervisor = supervisor_with_budget(2);
    let mut last_epoch = make_ready(&mut supervisor, "application-v1", 1);
    for attempt in 1_u64..=3 {
        supervisor
            .report_crash(format!("injected:terminal:{attempt}"), attempt * 100)
            .expect("inject terminal crash");
        if attempt < 3 {
            restart_when_due(&mut supervisor);
            last_epoch = make_ready(
                &mut supervisor,
                &format!("application-v{}", attempt + 1),
                u8::try_from(attempt + 1).expect("digest"),
            );
        }
    }
    assert_eq!(supervisor.phase(), VmShardPhase::Quarantined);
    assert_eq!(supervisor.restart_count(), 3);
    assert_eq!(supervisor.restart_deadline_tick(), None);
    assert!(!supervisor.is_routable());
    let terminal = supervisor.last_crash().expect("terminal crash report");
    assert_eq!(&terminal.shard_id, supervisor.shard_id());
    assert_eq!(terminal.epoch, Some(last_epoch));
    assert_eq!(terminal.reason, "injected:terminal:3");
    assert_eq!(terminal.observed_tick, 300);

    let quarantined = supervisor.clone();
    assert!(matches!(
        supervisor.restart_when_due(u64::MAX),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));
    assert!(matches!(
        supervisor.report_crash("late crash", 400),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));
    assert_eq!(supervisor, quarantined);
}
