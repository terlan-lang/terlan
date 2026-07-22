//! Tests for supervisor-owned execution-shard lifecycle semantics.

use super::execution_shard_test_support::{image, make_ready, protocol, shard_id, supervisor};
use super::*;
use crate::runtime::vm::{
    execution_shard_epoch::{
        VmShardEpochError, VmShardEpochOperation, VmShardOperationAdmission,
        VmShardOperationCommit, VmShardOperationId, VmShardOperationKind, VmShardReplayPolicy,
    },
    execution_shard_protocol::VmShardControlError,
    native_image_diagnostics::{
        VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
    },
};

/// Admission and readiness form the only transition into routability.
#[test]
fn shard_becomes_routable_only_after_admission_and_ready_acknowledgement() {
    let mut supervisor = supervisor();
    assert_eq!(supervisor.shard_id().as_str(), "primary");
    assert_eq!(supervisor.phase(), VmShardPhase::Created);
    assert!(!supervisor.is_routable());

    supervisor.begin_negotiation().expect("begin negotiation");
    assert!(!supervisor.is_routable());
    supervisor.negotiate(protocol(1)).expect("negotiate");
    let epoch = supervisor
        .admit_image(image("application-v1", 7))
        .expect("admit image");
    assert_eq!(epoch.as_u64(), 1);
    assert_eq!(supervisor.phase(), VmShardPhase::AwaitingReady);
    assert_eq!(supervisor.epoch(), Some(epoch));
    assert_eq!(
        supervisor.image().map(VmSealedShardImage::identity),
        Some("application-v1")
    );
    assert_eq!(supervisor.image().unwrap().descriptor_digest(), &[7; 32]);
    assert!(!supervisor.is_routable());

    supervisor
        .acknowledge_ready(epoch)
        .expect("ready acknowledgement");
    assert_eq!(supervisor.phase(), VmShardPhase::Ready);
    assert!(supervisor.is_routable());
}

/// Health and work progress require the active epoch and increasing sequences.
#[test]
fn signals_are_epoch_scoped_and_strictly_monotonic() {
    let mut supervisor = supervisor();
    let epoch = make_ready(&mut supervisor, "application-v1", 1);
    let stale = VmShardEpoch::new(99).expect("stale epoch");

    assert_eq!(
        supervisor.signal_health(stale, 1),
        Err(VmShardSupervisorError::EpochMismatch {
            expected: epoch,
            actual: stale,
        })
    );
    supervisor.signal_health(epoch, 1).expect("health signal");
    supervisor
        .signal_progress(epoch, 4)
        .expect("progress signal");
    assert_eq!(
        supervisor.signal_health(epoch, 1),
        Err(VmShardSupervisorError::NonMonotonicSignal {
            signal: "health",
            previous: 1,
            actual: 1,
        })
    );
    assert_eq!(
        supervisor.signals(),
        VmShardSignalProgress { health: 1, work: 4 }
    );

    supervisor.begin_drain(epoch).expect("begin drain");
    assert!(!supervisor.is_routable());
    supervisor
        .signal_progress(epoch, 5)
        .expect("draining progress");
}

/// Draining leads to a graceful terminal state with routing already revoked.
#[test]
fn graceful_stop_requires_drain_and_exact_epoch() {
    let mut supervisor = supervisor();
    let epoch = make_ready(&mut supervisor, "application-v1", 1);
    let wrong = VmShardEpoch::new(2).expect("wrong epoch");

    assert!(matches!(
        supervisor.request_graceful_stop(epoch),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));
    supervisor.begin_drain(epoch).expect("begin drain");
    assert_eq!(
        supervisor.request_graceful_stop(wrong),
        Err(VmShardSupervisorError::EpochMismatch {
            expected: epoch,
            actual: wrong,
        })
    );
    supervisor
        .request_graceful_stop(epoch)
        .expect("request stop");
    supervisor
        .acknowledge_stopped(epoch)
        .expect("acknowledge stop");
    assert_eq!(supervisor.phase(), VmShardPhase::Stopped);
    assert_eq!(supervisor.termination(), Some(VmShardTermination::Graceful));
    assert!(!supervisor.is_routable());
}

/// A deliberate replacement advances the epoch without consuming restart budget.
#[test]
fn drained_image_replacement_preserves_supervision_and_reopens_readiness() {
    let mut supervisor = supervisor();
    let original = make_ready(&mut supervisor, "application-v1", 1);
    supervisor.begin_drain(original).expect("drain original");
    let before_stale_replacement = supervisor.clone();
    let stale = VmShardEpoch::new(99).expect("stale epoch");
    assert_eq!(
        supervisor.replace_drained_image(stale, image("stale", 9)),
        Err(VmShardSupervisorError::EpochMismatch {
            expected: original,
            actual: stale,
        })
    );
    assert_eq!(supervisor, before_stale_replacement);

    let replacement = supervisor
        .replace_drained_image(original, image("application-v2", 2))
        .expect("install replacement");
    assert!(replacement > original);
    assert_eq!(supervisor.phase(), VmShardPhase::AwaitingReady);
    assert_eq!(supervisor.restart_count(), 0);
    assert!(!supervisor.is_routable());
    assert_eq!(
        supervisor.image().map(VmSealedShardImage::identity),
        Some("application-v2")
    );
    assert_eq!(
        supervisor.replace_drained_image(original, image("invalid", 3)),
        Err(VmShardSupervisorError::InvalidTransition {
            phase: VmShardPhase::AwaitingReady,
            operation: "replace_drained_image",
        })
    );

    supervisor
        .acknowledge_ready(replacement)
        .expect("publish replacement");
    assert!(supervisor.is_routable());
}

/// A drain timeout quarantines the generation without dropping its admitted image.
#[test]
fn drain_timeout_quarantines_and_retains_reachable_image() {
    let mut supervisor = supervisor();
    let epoch = make_ready(&mut supervisor, "application-v1", 1);
    supervisor.begin_drain(epoch).expect("begin drain");
    let mut references = VmNativeGenerationReferenceSnapshot::new();
    references.record(VmNativeGenerationReferenceClass::Debugger, 1);

    supervisor
        .quarantine_drain_timeout_with_lifetime(epoch, "debugger_pins=1", 50, &references)
        .expect("quarantine timed-out generation");

    assert_eq!(supervisor.phase(), VmShardPhase::Quarantined);
    assert_eq!(supervisor.epoch(), Some(epoch));
    assert_eq!(
        supervisor.image().map(VmSealedShardImage::identity),
        Some("application-v1")
    );
    assert_eq!(
        supervisor.last_crash().map(|report| report.reason.as_str()),
        Some("debugger_pins=1")
    );
    let native_image = supervisor
        .last_crash()
        .and_then(|report| report.native_image.as_ref())
        .expect("quarantine retains native image metadata");
    assert_eq!(native_image.image_identity, "application-v1");
    assert_eq!(native_image.generation_reference_total, 1);
    assert_eq!(native_image.generation_references[0].class, "debugger_pins");
    assert_eq!(
        supervisor.replace_drained_image(epoch, image("unsafe-replacement", 2)),
        Err(VmShardSupervisorError::InvalidTransition {
            phase: VmShardPhase::Quarantined,
            operation: "replace_drained_image",
        })
    );
}

/// Forced termination is explicit and cannot rewrite a terminal outcome.
#[test]
fn forced_termination_is_terminal_and_idempotence_fails_closed() {
    let mut supervisor = supervisor();
    supervisor.begin_negotiation().expect("begin negotiation");
    supervisor.force_terminate().expect("force termination");
    assert_eq!(supervisor.phase(), VmShardPhase::Stopped);
    assert_eq!(supervisor.termination(), Some(VmShardTermination::Forced));
    assert!(matches!(
        supervisor.force_terminate(),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));
}

/// Crashes consume restart budget, double their delay, and end in quarantine.
#[test]
fn crash_backoff_assigns_new_epochs_then_quarantines_terminally() {
    let mut supervisor = supervisor();
    let first_epoch = make_ready(&mut supervisor, "application-v1", 1);

    supervisor
        .report_crash("fault one", 100)
        .expect("first crash");
    assert_eq!(supervisor.phase(), VmShardPhase::RestartBackoff);
    assert_eq!(supervisor.restart_count(), 1);
    assert_eq!(supervisor.restart_deadline_tick(), Some(110));
    assert_eq!(supervisor.last_crash().unwrap().epoch, Some(first_epoch));
    assert!(!supervisor.is_routable());
    assert_eq!(
        supervisor.restart_when_due(109),
        Err(VmShardSupervisorError::RestartBackoffActive {
            deadline_tick: 110,
            now_tick: 109,
        })
    );

    supervisor.restart_when_due(110).expect("first restart");
    let second_epoch = make_ready(&mut supervisor, "application-v2", 2);
    assert_eq!(second_epoch.as_u64(), 2);
    supervisor
        .report_crash("fault two", 200)
        .expect("second crash");
    assert_eq!(supervisor.restart_deadline_tick(), Some(220));

    supervisor.restart_when_due(220).expect("second restart");
    let third_epoch = make_ready(&mut supervisor, "application-v3", 3);
    assert_eq!(third_epoch.as_u64(), 3);
    supervisor
        .report_crash("fault three", 300)
        .expect("budget exhaustion");
    assert_eq!(supervisor.phase(), VmShardPhase::Quarantined);
    assert_eq!(supervisor.restart_count(), 3);
    assert_eq!(supervisor.restart_deadline_tick(), None);
    assert!(matches!(
        supervisor.restart_when_due(u64::MAX),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));
    assert!(matches!(
        supervisor.force_terminate(),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));
}

/// Invalid protocol, image, crash, and timing inputs leave state unchanged.
#[test]
fn malformed_events_fail_without_partial_state_mutation() {
    assert_eq!(
        VmShardProtocolVersion::new(0),
        Err(VmShardSupervisorError::ZeroProtocolVersion)
    );
    let mut supervisor = supervisor();
    let snapshot = supervisor.clone();
    assert!(matches!(
        supervisor.acknowledge_ready(VmShardEpoch::new(1).unwrap()),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));
    assert_eq!(supervisor, snapshot);

    supervisor.begin_negotiation().expect("begin negotiation");
    let negotiating = supervisor.clone();
    assert_eq!(
        supervisor.negotiate(protocol(2)),
        Err(VmShardSupervisorError::ProtocolMismatch {
            expected: 1,
            actual: 2,
        })
    );
    assert_eq!(supervisor, negotiating);
    assert_eq!(
        VmSealedShardImage::new("", [1; 32]),
        Err(VmShardControlError::EmptyImageIdentity)
    );
    assert_eq!(
        VmSealedShardImage::new("image", [0; 32]),
        Err(VmShardControlError::EmptyImageDigest)
    );

    supervisor.negotiate(protocol(1)).expect("negotiate");
    supervisor
        .admit_image(image("application", 1))
        .expect("admit image");
    let admitted = supervisor.clone();
    assert_eq!(
        supervisor.report_crash("  ", 1),
        Err(VmShardSupervisorError::EmptyCrashReason)
    );
    assert_eq!(supervisor, admitted);

    let mut overflow = VmExecutionShardSupervisor::new(
        shard_id(),
        VmShardSupervisorPolicy::new(protocol(1), 1, VmRestartBackoffSchedule::exponential(2, 2)),
    );
    let before_overflow = overflow.clone();
    assert_eq!(
        overflow.report_crash("overflow", u64::MAX),
        Err(VmShardSupervisorError::RestartDeadlineOverflow)
    );
    assert_eq!(overflow, before_overflow);
}

/// Restart advances the operation fence and retains uncertain-effect evidence.
#[test]
fn supervisor_rejects_stale_operations_and_suppresses_uncertain_recovery() {
    let mut supervisor = supervisor();
    let first_epoch = make_ready(&mut supervisor, "application-v1", 1);
    let operation_id = VmShardOperationId::new(71).expect("operation identity");
    let first_attempt = VmShardEpochOperation::new(
        operation_id,
        first_epoch,
        VmShardOperationKind::CapabilityCompletion,
        VmShardReplayPolicy::AtMostOnce,
    );
    assert_eq!(
        supervisor.begin_epoch_operation(first_attempt),
        Ok(VmShardOperationAdmission::ExecuteFirst)
    );
    supervisor.report_crash("worker lost", 10).expect("crash");
    assert!(matches!(
        supervisor.begin_epoch_operation(first_attempt),
        Err(VmShardSupervisorError::InvalidTransition { .. })
    ));

    supervisor.restart_when_due(20).expect("restart due");
    let second_epoch = make_ready(&mut supervisor, "application-v2", 2);
    assert_eq!(
        supervisor.begin_epoch_operation(first_attempt),
        Err(VmShardSupervisorError::EpochOperation(
            VmShardEpochError::StaleEpoch {
                expected: second_epoch,
                actual: first_epoch,
            }
        ))
    );
    let recovery_attempt = VmShardEpochOperation {
        epoch: second_epoch,
        ..first_attempt
    };
    assert_eq!(
        supervisor.begin_epoch_operation(recovery_attempt),
        Ok(VmShardOperationAdmission::IndeterminateSuppressed)
    );

    let completed = VmShardEpochOperation::new(
        VmShardOperationId::new(72).expect("completed operation identity"),
        second_epoch,
        VmShardOperationKind::ResourceNotification,
        VmShardReplayPolicy::AtMostOnce,
    );
    assert_eq!(
        supervisor.begin_epoch_operation(completed),
        Ok(VmShardOperationAdmission::ExecuteFirst)
    );
    assert_eq!(
        supervisor.commit_epoch_operation(completed),
        Ok(VmShardOperationCommit::Committed)
    );
    assert_eq!(
        supervisor.begin_epoch_operation(completed),
        Ok(VmShardOperationAdmission::DuplicateSuppressed)
    );
}
