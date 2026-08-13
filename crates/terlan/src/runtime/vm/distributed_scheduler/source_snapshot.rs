use std::collections::BTreeMap;

use super::placement_override::validate_policy;
use super::{
    validate_scheduling_limits, VmDistributedFaultPolicy, VmDistributedFaultStatus,
    VmDistributedNodeLoad, VmDistributedScheduler, VmMigrationIntent, VmMigrationOutcome,
    VmMigrationPhase, VmPlacementAssignment, VmPlacementPolicy, VmSchedulerEvent,
    VmSchedulingLimits,
};

/// Deterministic source-boundary state for an immutable scheduler transition.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDistributedSchedulerSourceSnapshot {
    pub(crate) active_nodes: Vec<VmDistributedNodeLoad>,
    pub(crate) round_robin_cursor: usize,
    pub(crate) shard_owners: Vec<(String, String)>,
    pub(crate) route_policy_overrides: Vec<(String, VmPlacementPolicy)>,
    pub(crate) actor_group_policy_overrides: Vec<(String, String, VmPlacementPolicy)>,
    pub(crate) next_migration_sequence: u64,
    pub(crate) placement_assignments: Vec<VmPlacementAssignment>,
    pub(crate) in_flight_migrations: Vec<VmMigrationIntent>,
    pub(crate) completed_migration_outcomes: Vec<(String, u64, VmMigrationOutcome)>,
    pub(crate) next_event_sequence: u64,
    pub(crate) events: Vec<VmSchedulerEvent>,
    pub(crate) limits: VmSchedulingLimits,
    pub(crate) last_migration_tick: Option<u64>,
    pub(crate) migrations_by_tick: Vec<(u64, usize)>,
}

impl VmDistributedScheduler {
    /// Captures source-visible scheduler state without exposing runtime internals.
    #[cfg(test)]
    pub(crate) fn source_snapshot(&self) -> VmDistributedSchedulerSourceSnapshot {
        VmDistributedSchedulerSourceSnapshot {
            active_nodes: self.active_nodes.values().cloned().collect(),
            round_robin_cursor: self.round_robin_cursor,
            shard_owners: self
                .shard_owners
                .iter()
                .map(|(key, owner)| (key.clone(), owner.clone()))
                .collect(),
            route_policy_overrides: self
                .route_policy_overrides
                .iter()
                .map(|(route_id, policy)| (route_id.clone(), policy.clone()))
                .collect(),
            actor_group_policy_overrides: self
                .actor_group_policy_overrides
                .iter()
                .flat_map(|(route_id, groups)| {
                    groups.iter().map(|(group_id, policy)| {
                        (route_id.clone(), group_id.clone(), policy.clone())
                    })
                })
                .collect(),
            next_migration_sequence: self.next_migration_sequence,
            placement_assignments: self.placement_assignments.values().cloned().collect(),
            in_flight_migrations: self.in_flight_migrations.values().cloned().collect(),
            completed_migration_outcomes: self
                .completed_migration_outcomes
                .iter()
                .map(|((actor_id, sequence), outcome)| {
                    (actor_id.clone(), *sequence, outcome.clone())
                })
                .collect(),
            next_event_sequence: self.next_event_sequence,
            events: self.events.clone(),
            limits: self.limits,
            last_migration_tick: self.last_migration_tick,
            migrations_by_tick: self
                .migrations_by_tick
                .iter()
                .map(|(tick, count)| (*tick, *count))
                .collect(),
        }
    }

    /// Restores a scheduler from a validated immutable source transition.
    #[cfg(test)]
    pub(crate) fn from_source_snapshot(
        snapshot: VmDistributedSchedulerSourceSnapshot,
    ) -> Result<Self, String> {
        validate_scheduling_limits(snapshot.limits)?;
        let active_nodes = collect_unique_nodes(snapshot.active_nodes)?;
        if snapshot.round_robin_cursor >= active_nodes.len() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot round-robin cursor is out of bounds"
                    .to_string(),
            );
        }
        let shard_owners = collect_unique_pairs(snapshot.shard_owners, "shard owner")?;
        let route_policy_overrides =
            collect_route_policy_overrides(snapshot.route_policy_overrides)?;
        let actor_group_policy_overrides =
            collect_actor_group_policy_overrides(snapshot.actor_group_policy_overrides)?;
        let placement_assignments = collect_placements(snapshot.placement_assignments)?;
        let in_flight_migrations = collect_migrations(snapshot.in_flight_migrations)?;
        let completed_migration_outcomes = collect_outcomes(snapshot.completed_migration_outcomes)?;
        validate_event_sequences(snapshot.next_event_sequence, &snapshot.events)?;
        validate_migration_sequence(
            snapshot.next_migration_sequence,
            in_flight_migrations.values(),
            completed_migration_outcomes
                .keys()
                .map(|(_, sequence)| *sequence),
        )?;
        let migrations_by_tick = collect_tick_counts(snapshot.migrations_by_tick)?;
        let fault_states = active_nodes
            .keys()
            .map(|node_id| (node_id.clone(), VmDistributedFaultStatus::recovered()))
            .collect();
        let last_heartbeat_ticks = active_nodes
            .keys()
            .map(|node_id| (node_id.clone(), 0))
            .collect();
        Ok(Self {
            active_nodes,
            round_robin_cursor: snapshot.round_robin_cursor,
            shard_owners,
            route_policy_overrides,
            actor_group_policy_overrides,
            next_migration_sequence: snapshot.next_migration_sequence,
            placement_assignments,
            in_flight_migrations,
            completed_migration_outcomes,
            next_event_sequence: snapshot.next_event_sequence,
            events: snapshot.events,
            limits: snapshot.limits,
            last_migration_tick: snapshot.last_migration_tick,
            migrations_by_tick,
            fault_states,
            fault_transitions: Vec::new(),
            failure_envelopes: Vec::new(),
            fault_policy: VmDistributedFaultPolicy::default(),
            last_heartbeat_ticks,
        })
    }
}

#[cfg(test)]
fn collect_unique_nodes(
    nodes: Vec<VmDistributedNodeLoad>,
) -> Result<BTreeMap<String, VmDistributedNodeLoad>, String> {
    let mut collected = BTreeMap::new();
    for node in nodes {
        if node.node_id.is_empty() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot node id must be non-empty"
                    .to_string(),
            );
        }
        if collected.insert(node.node_id.clone(), node).is_some() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot contains duplicate nodes"
                    .to_string(),
            );
        }
    }
    if collected.is_empty() {
        return Err("error[vm_distributed_scheduler]: no active nodes available".to_string());
    }
    Ok(collected)
}

#[cfg(test)]
fn collect_unique_pairs(
    pairs: Vec<(String, String)>,
    label: &str,
) -> Result<BTreeMap<String, String>, String> {
    let mut collected = BTreeMap::new();
    for (key, value) in pairs {
        if key.is_empty() || value.is_empty() {
            return Err(format!(
                "error[vm_distributed_scheduler]: source snapshot {label} entries must be non-empty"
            ));
        }
        if collected.insert(key, value).is_some() {
            return Err(format!(
                "error[vm_distributed_scheduler]: source snapshot contains duplicate {label} entries"
            ));
        }
    }
    Ok(collected)
}

#[cfg(test)]
fn collect_route_policy_overrides(
    overrides: Vec<(String, VmPlacementPolicy)>,
) -> Result<BTreeMap<String, VmPlacementPolicy>, String> {
    let mut collected = BTreeMap::new();
    for (route_id, policy) in overrides {
        if route_id.is_empty() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot route id must be non-empty"
                    .to_string(),
            );
        }
        validate_policy(&policy)?;
        if collected.insert(route_id, policy).is_some() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot contains duplicate route policy overrides"
                    .to_string(),
            );
        }
    }
    Ok(collected)
}

#[cfg(test)]
fn collect_actor_group_policy_overrides(
    overrides: Vec<(String, String, VmPlacementPolicy)>,
) -> Result<BTreeMap<String, BTreeMap<String, VmPlacementPolicy>>, String> {
    let mut collected = BTreeMap::new();
    for (route_id, group_id, policy) in overrides {
        if route_id.is_empty() || group_id.is_empty() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot actor-group scope ids must be non-empty"
                    .to_string(),
            );
        }
        validate_policy(&policy)?;
        let route_groups = collected.entry(route_id).or_insert_with(BTreeMap::new);
        if route_groups.insert(group_id, policy).is_some() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot contains duplicate actor-group policy overrides"
                    .to_string(),
            );
        }
    }
    Ok(collected)
}

#[cfg(test)]
fn collect_placements(
    placements: Vec<VmPlacementAssignment>,
) -> Result<BTreeMap<String, VmPlacementAssignment>, String> {
    let mut collected = BTreeMap::new();
    for placement in placements {
        let actor_id = placement.decision.actor_id.clone();
        if actor_id.is_empty() || placement.event_sequence == 0 {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot placement is invalid".to_string(),
            );
        }
        if collected.insert(actor_id, placement).is_some() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot contains duplicate placements"
                    .to_string(),
            );
        }
    }
    Ok(collected)
}

#[cfg(test)]
fn collect_migrations(
    migrations: Vec<VmMigrationIntent>,
) -> Result<BTreeMap<String, VmMigrationIntent>, String> {
    let mut collected = BTreeMap::new();
    for migration in migrations {
        if migration.actor_id.is_empty() || migration.sequence == 0 {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot migration is invalid".to_string(),
            );
        }
        let readiness_is_valid = match migration.phase {
            VmMigrationPhase::Requested | VmMigrationPhase::Snapshotting => {
                migration.state_snapshot_ready != migration.stateful
                    && !migration.in_flight_messages_ready
            }
            VmMigrationPhase::Transferring => {
                migration.state_snapshot_ready && !migration.in_flight_messages_ready
            }
            VmMigrationPhase::Resuming => {
                migration.state_snapshot_ready && migration.in_flight_messages_ready
            }
        };
        if !readiness_is_valid {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot migration readiness is invalid"
                    .to_string(),
            );
        }
        if collected
            .insert(migration.actor_id.clone(), migration)
            .is_some()
        {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot contains duplicate migrations"
                    .to_string(),
            );
        }
    }
    Ok(collected)
}

#[cfg(test)]
fn collect_outcomes(
    outcomes: Vec<(String, u64, VmMigrationOutcome)>,
) -> Result<BTreeMap<(String, u64), VmMigrationOutcome>, String> {
    let mut collected = BTreeMap::new();
    for (actor_id, sequence, outcome) in outcomes {
        if actor_id.is_empty() || sequence == 0 {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot outcome is invalid".to_string(),
            );
        }
        if collected.insert((actor_id, sequence), outcome).is_some() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot contains duplicate outcomes"
                    .to_string(),
            );
        }
    }
    Ok(collected)
}

#[cfg(test)]
fn validate_event_sequences(next_sequence: u64, events: &[VmSchedulerEvent]) -> Result<(), String> {
    let mut previous = 0;
    for event in events {
        if event.event_sequence <= previous || event.event_sequence > next_sequence {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot event sequence is invalid"
                    .to_string(),
            );
        }
        previous = event.event_sequence;
    }
    if previous != next_sequence {
        return Err(
            "error[vm_distributed_scheduler]: source snapshot event cursor is inconsistent"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
fn validate_migration_sequence<'a>(
    next_sequence: u64,
    in_flight: impl Iterator<Item = &'a VmMigrationIntent>,
    completed: impl Iterator<Item = u64>,
) -> Result<(), String> {
    let maximum = in_flight
        .map(|migration| migration.sequence)
        .chain(completed)
        .max()
        .unwrap_or(0);
    if maximum > next_sequence {
        return Err(
            "error[vm_distributed_scheduler]: source snapshot migration cursor is inconsistent"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
fn collect_tick_counts(counts: Vec<(u64, usize)>) -> Result<BTreeMap<u64, usize>, String> {
    let mut collected = BTreeMap::new();
    for (tick, count) in counts {
        if count == 0 || collected.insert(tick, count).is_some() {
            return Err(
                "error[vm_distributed_scheduler]: source snapshot migration tick counts are invalid"
                    .to_string(),
            );
        }
    }
    Ok(collected)
}

#[cfg(test)]
#[path = "source_snapshot_test.rs"]
mod tests;
