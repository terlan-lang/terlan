use super::*;
use crate::runtime::vm::distributed_storage::{
    VmDistributedStorageAdapter, VmDistributedStorageOutcome, VmDistributedStoragePolicy,
    VmDistributedStorageSnapshot,
};
use crate::runtime::vm::{
    distributed_state::{
        VmDistributedStateEntry, VmDistributedStatePolicy, VmDistributedStateScope,
        VmDistributedStateVersion,
    },
    ReplValue,
};

fn corrupt_snapshot(checkpoint_id: &str, sequence: u64) -> VmDistributedStorageSnapshot {
    VmDistributedStorageSnapshot::with_checksum(
        checkpoint_id,
        sequence,
        vec![state_entry("node-a", "recovery", sequence as i64)],
        0,
    )
    .expect("corrupt snapshot descriptor should build")
}

fn state_entry(namespace: &str, key: &str, value: i64) -> VmDistributedStateEntry {
    VmDistributedStateEntry {
        scope: VmDistributedStateScope::new(namespace, key).expect("scope should be valid"),
        owner_node_id: "node-a".to_string(),
        value: ReplValue::Int(value),
        version: VmDistributedStateVersion::new(value as u64, "node-a")
            .expect("version should be valid"),
        policy: VmDistributedStatePolicy::WinnerTakesAll,
    }
}

#[test]
fn vm_distributed_scheduler_expires_recovery_window_with_failure_envelope() {
    let mut scheduler = VmDistributedScheduler::from_membership_with_limits_and_fault_policy(
        [
            node("node-a", VmClusterNodeState::Active),
            node("node-b", VmClusterNodeState::Active),
        ],
        VmSchedulingLimits::default(),
        VmDistributedFaultPolicy::new(2, 5, 9).expect("policy should build"),
    )
    .expect("scheduler should build");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect("suspect transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            11,
            "partition_confirmed",
        )
        .expect("isolation transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovering,
            12,
            "link_restored",
        )
        .expect("recovering transition should pass");

    assert_eq!(
        scheduler
            .expire_recovery_window_at_tick("node-a", 21, "recovery_window_expired")
            .expect("threshold edge should not expire"),
        None
    );
    let expired = scheduler
        .expire_recovery_window_at_tick("node-a", 22, "recovery_window_expired")
        .expect("recovery window should expire")
        .expect("threshold breach should emit transition");
    assert_eq!(
        expired,
        fault_transition(
            "node-a",
            VmDistributedFaultState::Recovering,
            VmDistributedFaultState::Isolated,
            22,
            "recovery_window_expired",
        )
    );
    assert_eq!(
        scheduler.failure_envelopes_after(21),
        vec![failure_envelope(
            "node-a",
            22,
            VmDistributedFailureKind::RecoveryWindowExpired {
                node_id: "node-a".to_string(),
                recovery_started_tick: 12,
                current_tick: 22,
                window_ticks: 9,
            },
            "recovery_window_expired",
        )]
    );
    let transition_count = scheduler.fault_transitions_after(0).len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();
    assert_eq!(
        scheduler
            .expire_recovery_window_at_tick("node-a", 22, "recovery_window_expired")
            .expect("duplicate expiry should replay"),
        Some(expired)
    );
    assert_eq!(scheduler.fault_transitions_after(0).len(), transition_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);
}

#[test]
fn vm_distributed_scheduler_bounds_recovery_expiry_after_storage_checksum_failure() {
    let mut scheduler = VmDistributedScheduler::from_membership_with_limits_and_fault_policy(
        [
            node("node-a", VmClusterNodeState::Active),
            node("node-b", VmClusterNodeState::Active),
        ],
        VmSchedulingLimits::default(),
        VmDistributedFaultPolicy::new(2, 5, 9).expect("policy should build"),
    )
    .expect("scheduler should build");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect("suspect transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            11,
            "partition_confirmed",
        )
        .expect("isolation transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovering,
            12,
            "storage_repair_started",
        )
        .expect("recovering transition should pass");

    let mut storage = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    assert!(matches!(
        storage.open(),
        VmDistributedStorageOutcome::Opened { .. }
    ));
    let storage_failure = storage.append(corrupt_snapshot("node-a.recovery", 1));
    assert!(matches!(
        storage_failure,
        VmDistributedStorageOutcome::ChecksumMismatch { .. }
    ));
    assert!(storage_failure.requires_recovery());
    assert_eq!(storage_failure.recovery_action(), "repair_snapshot");

    let reason = storage_failure.kind();
    let expired = scheduler
        .expire_recovery_window_at_tick("node-a", 22, reason)
        .expect("storage failure should allow bounded recovery expiry")
        .expect("threshold breach should emit transition");
    assert_eq!(
        expired,
        fault_transition(
            "node-a",
            VmDistributedFaultState::Recovering,
            VmDistributedFaultState::Isolated,
            22,
            "checksum_mismatch",
        )
    );
    assert_eq!(
        scheduler.failure_envelopes_after(21),
        vec![failure_envelope(
            "node-a",
            22,
            VmDistributedFailureKind::RecoveryWindowExpired {
                node_id: "node-a".to_string(),
                recovery_started_tick: 12,
                current_tick: 22,
                window_ticks: 9,
            },
            "checksum_mismatch",
        )]
    );

    let transition_count = scheduler.fault_transitions_after(0).len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();
    assert_eq!(
        scheduler
            .expire_recovery_window_at_tick("node-a", 22, reason)
            .expect("duplicate storage recovery expiry should replay"),
        Some(expired)
    );
    assert_eq!(scheduler.fault_transitions_after(0).len(), transition_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_recovery_expiry_inputs() {
    let mut scheduler = two_node_scheduler();
    let not_recovering = scheduler
        .expire_recovery_window_at_tick("node-a", 20, "recovery_window_expired")
        .expect_err("recovered node cannot expire recovery");
    assert_eq!(
        not_recovering,
        "error[vm_distributed_scheduler]: node `node-a` must be `Recovering` before recovery window expiry"
    );

    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect("suspect transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            11,
            "partition_confirmed",
        )
        .expect("isolation transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovering,
            12,
            "link_restored",
        )
        .expect("recovering transition should pass");

    let stale = scheduler
        .expire_recovery_window_at_tick("node-a", 9, "recovery_window_expired")
        .expect_err("stale recovery expiry should fail");
    assert_eq!(
        stale,
        "error[vm_distributed_scheduler]: recovery expiry tick `9` is older than recovery start tick `12` for node `node-a`"
    );
    let empty_reason = scheduler
        .expire_recovery_window_at_tick("node-a", 20, "")
        .expect_err("empty reason should fail");
    assert_eq!(
        empty_reason,
        "error[vm_distributed_scheduler]: fault transition reason must be non-empty"
    );
}

#[test]
fn vm_distributed_scheduler_completes_recovery_and_refreshes_heartbeat() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect("suspect transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            11,
            "partition_confirmed",
        )
        .expect("isolation transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovering,
            12,
            "link_restored",
        )
        .expect("recovering transition should pass");

    let recovered = scheduler
        .complete_recovery_at_tick("node-a", 15, "health_restored")
        .expect("recovery completion should pass");
    assert_eq!(
        recovered,
        fault_transition(
            "node-a",
            VmDistributedFaultState::Recovering,
            VmDistributedFaultState::Recovered,
            15,
            "health_restored",
        )
    );
    assert_eq!(
        scheduler.fault_state("node-a"),
        Some(VmDistributedFaultState::Recovered)
    );
    assert_eq!(
        scheduler
            .record_fault_heartbeat_at_tick("node-a", 15)
            .expect("same-tick heartbeat should be duplicate"),
        VmDistributedHeartbeatObservation::DuplicateSuppressed {
            node_id: "node-a".to_string(),
            tick: 15,
        }
    );
    let stale = scheduler
        .record_fault_heartbeat_at_tick("node-a", 14)
        .expect_err("pre-completion heartbeat should be stale");
    assert_eq!(
        stale,
        "error[vm_distributed_scheduler]: stale heartbeat tick `14` is older than last heartbeat tick `15` for node `node-a`"
    );
    let transition_count = scheduler.fault_transitions_after(0).len();
    assert_eq!(
        scheduler
            .complete_recovery_at_tick("node-a", 15, "health_restored")
            .expect("duplicate recovery completion should replay"),
        recovered
    );
    assert_eq!(scheduler.fault_transitions_after(0).len(), transition_count);
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_recovery_completion_inputs() {
    let mut scheduler = two_node_scheduler();
    let not_recovering = scheduler
        .complete_recovery_at_tick("node-a", 20, "health_restored")
        .expect_err("recovered node cannot complete recovery");
    assert_eq!(
        not_recovering,
        "error[vm_distributed_scheduler]: node `node-a` must be `Recovering` before recovery completion"
    );

    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect("suspect transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            11,
            "partition_confirmed",
        )
        .expect("isolation transition should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovering,
            12,
            "link_restored",
        )
        .expect("recovering transition should pass");

    let stale = scheduler
        .complete_recovery_at_tick("node-a", 9, "health_restored")
        .expect_err("stale recovery completion should fail");
    assert_eq!(
        stale,
        "error[vm_distributed_scheduler]: recovery completion tick `9` is older than recovery start tick `12` for node `node-a`"
    );
    let empty_reason = scheduler
        .complete_recovery_at_tick("node-a", 20, "")
        .expect_err("empty reason should fail");
    assert_eq!(
        empty_reason,
        "error[vm_distributed_scheduler]: fault transition reason must be non-empty"
    );
}
