use super::*;
use crate::runtime::vm::distributed_scheduler::fault::{
    distributed_fault_compatibility, VmDistributedCompatibilityOutcome,
};

#[test]
fn vm_distributed_scheduler_validates_explicit_fault_policy_thresholds() {
    let zero_suspicion =
        VmDistributedFaultPolicy::new(0, 6, 12).expect_err("zero suspicion threshold should fail");
    assert_eq!(
        zero_suspicion,
        "error[vm_distributed_scheduler]: suspicion threshold ticks must be non-zero"
    );
    let unordered_thresholds = VmDistributedFaultPolicy::new(3, 3, 12)
        .expect_err("isolation threshold must exceed suspicion threshold");
    assert_eq!(
        unordered_thresholds,
        "error[vm_distributed_scheduler]: isolation threshold `3` must be greater than suspicion threshold `3`"
    );
    let zero_recovery_window =
        VmDistributedFaultPolicy::new(3, 6, 0).expect_err("zero recovery window should fail");
    assert_eq!(
        zero_recovery_window,
        "error[vm_distributed_scheduler]: recovery window ticks must be non-zero"
    );
    let unordered_recovery_window = VmDistributedFaultPolicy::new(3, 6, 6)
        .expect_err("recovery window must exceed isolation threshold");
    assert_eq!(
        unordered_recovery_window,
        "error[vm_distributed_scheduler]: recovery window `6` must be greater than isolation threshold `6`"
    );

    let scheduler = VmDistributedScheduler::from_membership_with_limits_and_fault_policy(
        [
            node("node-a", VmClusterNodeState::Active),
            node("node-b", VmClusterNodeState::Active),
        ],
        VmSchedulingLimits::default(),
        VmDistributedFaultPolicy::new(2, 5, 9).expect("policy should build"),
    )
    .expect("scheduler should build with explicit policy");
    assert_eq!(
        scheduler.fault_policy(),
        VmDistributedFaultPolicy {
            suspicion_threshold_ticks: 2,
            isolation_threshold_ticks: 5,
            recovery_window_ticks: 9,
        }
    );
}

#[test]
fn vm_distributed_scheduler_records_fault_heartbeats_and_suppresses_duplicates() {
    let mut scheduler = two_node_scheduler();
    assert_eq!(
        scheduler
            .record_fault_heartbeat_at_tick("node-a", 2)
            .expect("heartbeat should record"),
        VmDistributedHeartbeatObservation::Recorded {
            node_id: "node-a".to_string(),
            tick: 2,
        }
    );
    assert_eq!(
        scheduler
            .record_fault_heartbeat_at_tick("node-a", 2)
            .expect("duplicate heartbeat should suppress"),
        VmDistributedHeartbeatObservation::DuplicateSuppressed {
            node_id: "node-a".to_string(),
            tick: 2,
        }
    );
    let stale = scheduler
        .record_fault_heartbeat_at_tick("node-a", 1)
        .expect_err("stale heartbeat should fail");
    assert_eq!(
        stale,
        "error[vm_distributed_scheduler]: stale heartbeat tick `1` is older than last heartbeat tick `2` for node `node-a`"
    );
}

#[test]
fn vm_distributed_scheduler_resolves_mismatched_fault_policies_and_compatibility_explicitly() {
    let local = VmDistributedFaultPolicy::new(2, 5, 9).expect("local policy");
    let peer = VmDistributedFaultPolicy::new(3, 7, 12).expect("peer policy");
    assert_eq!(
        local.resolve(peer),
        VmDistributedFaultPolicy {
            suspicion_threshold_ticks: 3,
            isolation_threshold_ticks: 7,
            recovery_window_ticks: 12,
        }
    );
    assert_eq!(
        distributed_fault_compatibility(true, false),
        VmDistributedCompatibilityOutcome::Supported
    );
    assert_eq!(
        distributed_fault_compatibility(false, true),
        VmDistributedCompatibilityOutcome::FallbackLocalOnly
    );
    assert_eq!(
        distributed_fault_compatibility(false, false),
        VmDistributedCompatibilityOutcome::FeatureUnsupported
    );
}

#[test]
fn vm_distributed_scheduler_replays_out_of_order_cross_node_fault_events_deterministically() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .suspect_missed_heartbeat_at_tick("node-b", 10, "node-b heartbeat")
        .expect("node-b suspicion");
    let node_a = scheduler
        .suspect_missed_heartbeat_at_tick("node-a", 5, "node-a heartbeat")
        .expect("node-a suspicion")
        .expect("node-a transition");
    scheduler
        .suspect_missed_heartbeat_at_tick("node-a", 5, "node-a heartbeat")
        .expect("duplicate recovery event replays");

    let transitions = scheduler.fault_transitions_after(0);
    assert_eq!(transitions[0], node_a);
    assert_eq!(transitions[0].tick, 5);
    assert_eq!(transitions[1].tick, 10);
    let failures = scheduler.failure_envelopes_after(0);
    assert_eq!(failures[0].tick, 5);
    assert_eq!(failures[1].tick, 10);
}

#[test]
fn vm_distributed_scheduler_bounds_partition_oscillation_and_rejects_stale_rejoin() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            5,
            "partition onset",
        )
        .expect("suspected");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            6,
            "role demoted",
        )
        .expect("isolated");
    scheduler
        .transition_fault_state_at_tick("node-a", VmDistributedFaultState::Recovering, 7, "rejoin")
        .expect("recovering");
    assert!(scheduler
        .complete_recovery_at_tick("node-a", 4, "stale rejoin")
        .expect_err("stale rejoin must fail")
        .contains("older than recovery start"));
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            8,
            "partition returned",
        )
        .expect("oscillation remains explicit");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovering,
            9,
            "stable rejoin",
        )
        .expect("second recovery");
    let recovered = scheduler
        .complete_recovery_at_tick("node-a", 10, "caught up")
        .expect("bounded recovery");
    assert_eq!(recovered.diagnostic_kind(), "recovery_completion");
}

#[test]
fn vm_distributed_scheduler_suspects_missed_heartbeat_once_with_failure_envelope() {
    let mut scheduler = two_node_scheduler();
    assert_eq!(
        scheduler
            .suspect_missed_heartbeat_at_tick("node-a", 4, "heartbeat_gap")
            .expect("threshold edge should not suspect"),
        None
    );

    let suspected = scheduler
        .suspect_missed_heartbeat_at_tick("node-a", 5, "heartbeat_gap")
        .expect("missed heartbeat should suspect")
        .expect("threshold breach should emit transition");
    assert_eq!(
        suspected,
        fault_transition(
            "node-a",
            VmDistributedFaultState::Recovered,
            VmDistributedFaultState::Suspected,
            5,
            "heartbeat_gap",
        )
    );
    assert_eq!(
        scheduler.failure_envelopes_after(4),
        vec![failure_envelope(
            "node-a",
            5,
            VmDistributedFailureKind::HeartbeatMissed {
                node_id: "node-a".to_string(),
                last_heartbeat_tick: 1,
                current_tick: 5,
            },
            "heartbeat_gap",
        )]
    );
    let transition_count = scheduler.fault_transitions_after(0).len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();
    assert_eq!(
        scheduler
            .suspect_missed_heartbeat_at_tick("node-a", 5, "heartbeat_gap")
            .expect("duplicate suspicion should replay"),
        Some(suspected)
    );
    assert_eq!(scheduler.fault_transitions_after(0).len(), transition_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);
}

#[test]
fn vm_distributed_scheduler_isolates_suspected_node_after_heartbeat_threshold() {
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
        .suspect_missed_heartbeat_at_tick("node-a", 4, "heartbeat_gap")
        .expect("missed heartbeat should suspect");

    assert_eq!(
        scheduler
            .isolate_missed_heartbeat_at_tick("node-a", 6, "partition_confirmed")
            .expect("threshold edge should not isolate"),
        None
    );
    let isolated = scheduler
        .isolate_missed_heartbeat_at_tick("node-a", 7, "partition_confirmed")
        .expect("partition should isolate")
        .expect("threshold breach should emit transition");
    assert_eq!(
        isolated,
        fault_transition(
            "node-a",
            VmDistributedFaultState::Suspected,
            VmDistributedFaultState::Isolated,
            7,
            "partition_confirmed",
        )
    );
    assert_eq!(
        scheduler.failure_envelopes_after(6),
        vec![failure_envelope(
            "node-a",
            7,
            VmDistributedFailureKind::PartitionSuspected {
                node_id: "node-a".to_string(),
                last_heartbeat_tick: 1,
                current_tick: 7,
                gap_ticks: 6,
            },
            "partition_confirmed",
        )]
    );
    let transition_count = scheduler.fault_transitions_after(0).len();
    let envelope_count = scheduler.failure_envelopes_after(0).len();
    assert_eq!(
        scheduler
            .isolate_missed_heartbeat_at_tick("node-a", 7, "partition_confirmed")
            .expect("duplicate partition should replay"),
        Some(isolated)
    );
    assert_eq!(scheduler.fault_transitions_after(0).len(), transition_count);
    assert_eq!(scheduler.failure_envelopes_after(0).len(), envelope_count);
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_heartbeat_isolation_inputs() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .record_fault_heartbeat_at_tick("node-a", 4)
        .expect("heartbeat should record");

    let stale = scheduler
        .isolate_missed_heartbeat_at_tick("node-a", 3, "stale_partition")
        .expect_err("stale isolation tick should fail");
    assert_eq!(
        stale,
        "error[vm_distributed_scheduler]: heartbeat isolation tick `3` is older than last heartbeat tick `4` for node `node-a`"
    );
    let not_suspected = scheduler
        .isolate_missed_heartbeat_at_tick("node-a", 11, "partition_confirmed")
        .expect_err("recovered node cannot isolate directly");
    assert_eq!(
        not_suspected,
        "error[vm_distributed_scheduler]: node `node-a` must be `Suspected` or `Degraded` before isolation"
    );
}

#[test]
fn vm_distributed_scheduler_tracks_legal_fault_state_transitions() {
    let mut scheduler = two_node_scheduler();

    assert_eq!(
        scheduler.fault_state("node-a"),
        Some(VmDistributedFaultState::Recovered)
    );
    assert_eq!(
        scheduler
            .transition_fault_state_at_tick(
                "node-a",
                VmDistributedFaultState::Suspected,
                10,
                "heartbeat_missed",
            )
            .expect("suspect transition should pass"),
        fault_transition(
            "node-a",
            VmDistributedFaultState::Recovered,
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
    );
    assert_eq!(
        scheduler
            .transition_fault_state_at_tick(
                "node-a",
                VmDistributedFaultState::Degraded,
                11,
                "quorum_weak",
            )
            .expect("degraded transition should pass")
            .previous_state,
        VmDistributedFaultState::Suspected
    );
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            12,
            "partition_confirmed",
        )
        .expect("isolation should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovering,
            13,
            "link_restored",
        )
        .expect("recovering should pass");
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Recovered,
            14,
            "health_restored",
        )
        .expect("recovered should pass");

    assert_eq!(
        scheduler.fault_state("node-a"),
        Some(VmDistributedFaultState::Recovered)
    );
    assert_eq!(scheduler.fault_transitions_after(11).len(), 3);
    assert_eq!(scheduler.fault_transitions_after(14), Vec::new());
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_fault_transitions_and_inputs() {
    let mut scheduler = two_node_scheduler();

    let invalid_jump = scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            10,
            "skip_suspicion",
        )
        .expect_err("recovered cannot jump directly to isolated");
    assert_eq!(
        invalid_jump,
        "error[vm_distributed_scheduler]: invalid fault transition for node `node-a` from `Recovered` to `Isolated`"
    );

    let missing_node = scheduler
        .transition_fault_state_at_tick(
            "node-missing",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect_err("unknown node should fail");
    assert_eq!(
        missing_node,
        "error[vm_distributed_scheduler]: fault node `node-missing` is not active"
    );

    let empty_reason = scheduler
        .transition_fault_state_at_tick("node-a", VmDistributedFaultState::Suspected, 10, "")
        .expect_err("empty fault reason should fail");
    assert_eq!(
        empty_reason,
        "error[vm_distributed_scheduler]: fault transition reason must be non-empty"
    );
}

#[test]
fn vm_distributed_scheduler_enforces_fault_tick_monotonicity_and_refreshes_membership() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect("first fault transition should pass");
    let stale_tick = scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Degraded,
            9,
            "stale_quorum",
        )
        .expect_err("stale tick should fail");
    assert_eq!(
        stale_tick,
        "error[vm_distributed_scheduler]: fault tick `9` is older than last fault tick `10` for node `node-a`"
    );

    scheduler
        .refresh_membership([
            node("node-a", VmClusterNodeState::Unreachable),
            node("node-b", VmClusterNodeState::Active),
            node("node-c", VmClusterNodeState::Active),
        ])
        .expect("membership should refresh");

    assert_eq!(scheduler.fault_state("node-a"), None);
    assert_eq!(
        scheduler.fault_state("node-b"),
        Some(VmDistributedFaultState::Recovered)
    );
    assert_eq!(
        scheduler.fault_state("node-c"),
        Some(VmDistributedFaultState::Recovered)
    );
}

#[test]
fn vm_distributed_scheduler_replays_duplicate_fault_transitions_without_duplicate_diagnostics() {
    let mut scheduler = two_node_scheduler();
    let suspected = scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Suspected,
            10,
            "heartbeat_missed",
        )
        .expect("suspect transition should pass");
    let transition_count = scheduler.fault_transitions_after(0).len();

    assert_eq!(
        scheduler
            .transition_fault_state_at_tick(
                "node-a",
                VmDistributedFaultState::Suspected,
                10,
                "heartbeat_missed",
            )
            .expect("duplicate latest transition should replay"),
        suspected
    );
    assert_eq!(scheduler.fault_transitions_after(0).len(), transition_count);

    scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Degraded,
            11,
            "quorum_weak",
        )
        .expect("degraded transition should pass");
    let transition_count = scheduler.fault_transitions_after(0).len();
    assert_eq!(
        scheduler
            .transition_fault_state_at_tick(
                "node-a",
                VmDistributedFaultState::Suspected,
                10,
                "heartbeat_missed",
            )
            .expect("out-of-order duplicate transition should replay"),
        suspected
    );
    assert_eq!(
        scheduler.fault_state("node-a"),
        Some(VmDistributedFaultState::Degraded)
    );
    assert_eq!(scheduler.fault_transitions_after(0).len(), transition_count);
}

#[test]
fn vm_distributed_scheduler_rejects_conflicting_duplicate_fault_transitions() {
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
            VmDistributedFaultState::Degraded,
            11,
            "quorum_weak",
        )
        .expect("degraded transition should pass");

    let conflicting = scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            10,
            "partition_confirmed",
        )
        .expect_err("conflicting duplicate tick should fail");
    assert_eq!(
        conflicting,
        "error[vm_distributed_scheduler]: fault transition for node `node-a` tick `10` already recorded as `Suspected` with reason `heartbeat_missed`"
    );
    let stale_unrecorded = scheduler
        .transition_fault_state_at_tick(
            "node-a",
            VmDistributedFaultState::Isolated,
            9,
            "stale_partition",
        )
        .expect_err("unrecorded stale tick should fail");
    assert_eq!(
        stale_unrecorded,
        "error[vm_distributed_scheduler]: fault tick `9` is older than last fault tick `11` for node `node-a`"
    );
}
