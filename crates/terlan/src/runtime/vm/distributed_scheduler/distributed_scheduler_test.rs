use super::{
    VmDistributedFailureEnvelope, VmDistributedFailureKind, VmDistributedFaultPolicy,
    VmDistributedFaultState, VmDistributedFaultTransition, VmDistributedHeartbeatObservation,
    VmDistributedScheduler, VmMigrationIntent, VmMigrationOutcome, VmMigrationPhase,
    VmPlacementAssignment, VmPlacementDecision, VmPlacementFallback, VmPlacementPolicy,
    VmSchedulerEvent, VmSchedulerEventKind, VmSchedulingLimits,
};
use crate::runtime::vm::coordination::{VmClusterNodeSnapshot, VmClusterNodeState};

#[cfg(test)]
#[path = "distributed_scheduler_test/fault.rs"]
mod fault;
#[cfg(test)]
#[path = "distributed_scheduler_test/migration.rs"]
mod migration;
#[cfg(test)]
#[path = "distributed_scheduler_test/placement.rs"]
mod placement;
#[cfg(test)]
#[path = "distributed_scheduler_test/placement_override.rs"]
mod placement_override;
#[cfg(test)]
#[path = "distributed_scheduler_test/pool_parity.rs"]
mod pool_parity;
#[cfg(test)]
#[path = "distributed_scheduler_test/recovery.rs"]
mod recovery;

fn decision(
    actor_id: &str,
    node_id: &str,
    policy: &'static str,
    fallback_used: bool,
) -> VmPlacementDecision {
    VmPlacementDecision {
        actor_id: actor_id.to_string(),
        node_id: node_id.to_string(),
        policy,
        fallback_used,
    }
}

fn assignment(
    actor_id: &str,
    node_id: &str,
    policy: &'static str,
    fallback_used: bool,
    event_sequence: u64,
) -> VmPlacementAssignment {
    VmPlacementAssignment {
        decision: decision(actor_id, node_id, policy, fallback_used),
        event_sequence,
    }
}

fn fault_transition(
    node_id: &str,
    previous_state: VmDistributedFaultState,
    next_state: VmDistributedFaultState,
    tick: u64,
    reason: &str,
) -> VmDistributedFaultTransition {
    VmDistributedFaultTransition {
        node_id: node_id.to_string(),
        previous_state,
        next_state,
        tick,
        reason: reason.to_string(),
    }
}

fn failure_envelope(
    node_id: &str,
    tick: u64,
    kind: VmDistributedFailureKind,
    reason: &str,
) -> VmDistributedFailureEnvelope {
    VmDistributedFailureEnvelope {
        node_id: node_id.to_string(),
        tick,
        kind,
        reason: reason.to_string(),
    }
}

fn node(node_id: &str, state: VmClusterNodeState) -> VmClusterNodeSnapshot {
    VmClusterNodeSnapshot {
        app_id: "app".to_string(),
        vm_id: node_id.replace("node", "vm"),
        node_id: node_id.to_string(),
        state,
        last_seen_tick: 1,
        role_tags: vec!["worker".to_string()],
    }
}

fn two_node_scheduler() -> VmDistributedScheduler {
    VmDistributedScheduler::from_membership([
        node("node-a", VmClusterNodeState::Active),
        node("node-b", VmClusterNodeState::Active),
    ])
    .expect("scheduler should build")
}

fn limited_scheduler(limits: VmSchedulingLimits) -> VmDistributedScheduler {
    VmDistributedScheduler::from_membership_with_limits(
        [
            node("node-a", VmClusterNodeState::Active),
            node("node-b", VmClusterNodeState::Active),
        ],
        limits,
    )
    .expect("limited scheduler should build")
}
