#![allow(dead_code)]

use std::collections::BTreeMap;

use super::coordination::{VmClusterNodeSnapshot, VmClusterNodeState};

mod fault;
mod placement_override;
mod source_snapshot;

#[allow(unused_imports)]
pub(crate) use fault::{
    distributed_fault_compatibility, VmDistributedFailureEnvelope, VmDistributedFailureKind,
    VmDistributedFaultPolicy, VmDistributedFaultState, VmDistributedFaultTransition,
    VmDistributedHeartbeatObservation,
};
use fault::{validate_fault_policy, VmDistributedFaultStatus};

/// Fallback behavior when a shard-affinity owner is unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmPlacementFallback {
    Reject,
    RoundRobin,
}

/// VM-owned distributed placement policy descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmPlacementPolicy {
    RoundRobin,
    LeastConnections,
    Pinned {
        node_id: String,
    },
    ShardAffinity {
        shard_key: String,
        fallback: VmPlacementFallback,
    },
}

/// Current scheduling load for one active VM cluster node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedNodeLoad {
    pub(crate) node_id: String,
    pub(crate) active_connections: usize,
}

/// Deterministic placement decision produced by the VM distributed scheduler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPlacementDecision {
    pub(crate) actor_id: String,
    pub(crate) node_id: String,
    pub(crate) policy: &'static str,
    pub(crate) fallback_used: bool,
}

/// Current placement assignment for one actor plus the event that established it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPlacementAssignment {
    pub(crate) decision: VmPlacementDecision,
    pub(crate) event_sequence: u64,
}

/// Ordered phase for one VM actor/process migration handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum VmMigrationPhase {
    Requested,
    Snapshotting,
    Transferring,
    Resuming,
}

/// Intent to move one actor/process between active VM cluster nodes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmMigrationIntent {
    pub(crate) actor_id: String,
    pub(crate) from_node_id: String,
    pub(crate) to_node_id: String,
    pub(crate) sequence: u64,
    pub(crate) stateful: bool,
    pub(crate) phase: VmMigrationPhase,
    pub(crate) state_snapshot_ready: bool,
    pub(crate) in_flight_messages_ready: bool,
}

/// Terminal outcome for one in-flight VM migration intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmMigrationOutcome {
    Committed { sequence: u64 },
    RolledBack { sequence: u64, reason: String },
    Aborted { sequence: u64, reason: String },
}

/// Typed VM scheduler event payload for placement and migration notifications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSchedulerEventKind {
    Placement {
        node_id: String,
        policy: &'static str,
        fallback_used: bool,
    },
    MigrationRequested {
        from_node_id: String,
        to_node_id: String,
        migration_sequence: u64,
        stateful: bool,
    },
    MigrationPhaseAdvanced {
        migration_sequence: u64,
        phase: VmMigrationPhase,
    },
    MigrationCommitted {
        migration_sequence: u64,
    },
    MigrationRolledBack {
        migration_sequence: u64,
        reason: String,
    },
    MigrationAborted {
        migration_sequence: u64,
        reason: String,
    },
}

/// Inspectable scheduler notification emitted by VM-owned scheduling decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSchedulerEvent {
    pub(crate) event_sequence: u64,
    pub(crate) actor_id: String,
    pub(crate) kind: VmSchedulerEventKind,
}

/// VM-owned scheduling limits for migration pressure and rebalance cadence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmSchedulingLimits {
    pub(crate) max_in_flight_migrations: usize,
    pub(crate) max_migrations_per_tick: usize,
    pub(crate) min_migration_interval_ticks: u64,
}

impl Default for VmSchedulingLimits {
    /// Returns conservative default scheduling limits for local VM tests.
    fn default() -> Self {
        Self {
            max_in_flight_migrations: 1024,
            max_migrations_per_tick: 1024,
            min_migration_interval_ticks: 0,
        }
    }
}

/// VM-owned distributed scheduler for actor/process placement decisions.
///
/// Inputs:
/// - Active cluster membership snapshots.
/// - Explicit placement policies and node load updates.
///
/// Output:
/// - Deterministic placement decisions with stable fallback metadata.
///
/// Transformation:
/// - Filters membership to active nodes, tracks local load, and resolves
///   policies without depending on external runtimes or transport internals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedScheduler {
    active_nodes: BTreeMap<String, VmDistributedNodeLoad>,
    round_robin_cursor: usize,
    shard_owners: BTreeMap<String, String>,
    route_policy_overrides: BTreeMap<String, VmPlacementPolicy>,
    actor_group_policy_overrides: BTreeMap<String, BTreeMap<String, VmPlacementPolicy>>,
    next_migration_sequence: u64,
    placement_assignments: BTreeMap<String, VmPlacementAssignment>,
    in_flight_migrations: BTreeMap<String, VmMigrationIntent>,
    completed_migration_outcomes: BTreeMap<(String, u64), VmMigrationOutcome>,
    next_event_sequence: u64,
    events: Vec<VmSchedulerEvent>,
    limits: VmSchedulingLimits,
    last_migration_tick: Option<u64>,
    migrations_by_tick: BTreeMap<u64, usize>,
    fault_states: BTreeMap<String, VmDistributedFaultStatus>,
    fault_transitions: Vec<VmDistributedFaultTransition>,
    failure_envelopes: Vec<VmDistributedFailureEnvelope>,
    fault_policy: VmDistributedFaultPolicy,
    last_heartbeat_ticks: BTreeMap<String, u64>,
}

impl VmDistributedScheduler {
    /// Builds a scheduler from the active portion of a membership view.
    pub(crate) fn from_membership(
        snapshots: impl IntoIterator<Item = VmClusterNodeSnapshot>,
    ) -> Result<Self, String> {
        Self::from_membership_with_limits(snapshots, VmSchedulingLimits::default())
    }

    /// Builds a scheduler with explicit migration pressure limits.
    pub(crate) fn from_membership_with_limits(
        snapshots: impl IntoIterator<Item = VmClusterNodeSnapshot>,
        limits: VmSchedulingLimits,
    ) -> Result<Self, String> {
        Self::from_membership_with_limits_and_fault_policy(
            snapshots,
            limits,
            VmDistributedFaultPolicy::default(),
        )
    }

    /// Builds a scheduler with explicit migration and fault-detection limits.
    pub(crate) fn from_membership_with_limits_and_fault_policy(
        snapshots: impl IntoIterator<Item = VmClusterNodeSnapshot>,
        limits: VmSchedulingLimits,
        fault_policy: VmDistributedFaultPolicy,
    ) -> Result<Self, String> {
        validate_scheduling_limits(limits)?;
        validate_fault_policy(fault_policy)?;
        let mut active_nodes = BTreeMap::new();
        let mut last_heartbeat_ticks = BTreeMap::new();
        for snapshot in snapshots {
            if snapshot.state == VmClusterNodeState::Active {
                let node_id = snapshot.node_id.clone();
                last_heartbeat_ticks.insert(node_id.clone(), snapshot.last_seen_tick);
                active_nodes.insert(
                    node_id,
                    VmDistributedNodeLoad {
                        node_id: snapshot.node_id,
                        active_connections: 0,
                    },
                );
            }
        }
        if active_nodes.is_empty() {
            return Err("error[vm_distributed_scheduler]: no active nodes available".to_string());
        }
        let fault_states = active_nodes
            .keys()
            .map(|node_id| (node_id.clone(), VmDistributedFaultStatus::recovered()))
            .collect();
        Ok(Self {
            active_nodes,
            round_robin_cursor: 0,
            shard_owners: BTreeMap::new(),
            route_policy_overrides: BTreeMap::new(),
            actor_group_policy_overrides: BTreeMap::new(),
            next_migration_sequence: 0,
            placement_assignments: BTreeMap::new(),
            in_flight_migrations: BTreeMap::new(),
            completed_migration_outcomes: BTreeMap::new(),
            next_event_sequence: 0,
            events: Vec::new(),
            limits,
            last_migration_tick: None,
            migrations_by_tick: BTreeMap::new(),
            fault_states,
            fault_transitions: Vec::new(),
            failure_envelopes: Vec::new(),
            fault_policy,
            last_heartbeat_ticks,
        })
    }

    /// Updates the active connection count for one active node.
    pub(crate) fn update_load(
        &mut self,
        node_id: &str,
        active_connections: usize,
    ) -> Result<(), String> {
        let load = self.active_nodes.get_mut(node_id).ok_or_else(|| {
            format!("error[vm_distributed_scheduler]: unknown active node `{node_id}`")
        })?;
        load.active_connections = active_connections;
        Ok(())
    }

    /// Refreshes active nodes from a new membership view.
    pub(crate) fn refresh_membership(
        &mut self,
        snapshots: impl IntoIterator<Item = VmClusterNodeSnapshot>,
    ) -> Result<(), String> {
        let mut active_nodes = BTreeMap::new();
        let mut refreshed_heartbeat_ticks = BTreeMap::new();
        for snapshot in snapshots {
            if snapshot.state == VmClusterNodeState::Active {
                let active_connections = self
                    .active_nodes
                    .get(&snapshot.node_id)
                    .map(|load| load.active_connections)
                    .unwrap_or(0);
                let node_id = snapshot.node_id.clone();
                let last_heartbeat_tick = self
                    .last_heartbeat_ticks
                    .get(&node_id)
                    .copied()
                    .unwrap_or(0)
                    .max(snapshot.last_seen_tick);
                refreshed_heartbeat_ticks.insert(node_id.clone(), last_heartbeat_tick);
                active_nodes.insert(
                    node_id,
                    VmDistributedNodeLoad {
                        node_id: snapshot.node_id,
                        active_connections,
                    },
                );
            }
        }
        if active_nodes.is_empty() {
            return Err("error[vm_distributed_scheduler]: no active nodes available".to_string());
        }
        self.fault_states
            .retain(|node_id, _| active_nodes.contains_key(node_id));
        for node_id in active_nodes.keys() {
            self.fault_states
                .entry(node_id.clone())
                .or_insert(VmDistributedFaultStatus::recovered());
        }
        self.last_heartbeat_ticks = refreshed_heartbeat_ticks;
        self.active_nodes = active_nodes;
        self.round_robin_cursor %= self.active_nodes.len();
        Ok(())
    }

    /// Places one actor according to an explicit VM-owned placement policy.
    pub(crate) fn place(
        &mut self,
        actor_id: impl Into<String>,
        policy: &VmPlacementPolicy,
    ) -> Result<VmPlacementDecision, String> {
        let actor_id = actor_id.into();
        self.validate_actor_id(&actor_id)?;
        match policy {
            VmPlacementPolicy::RoundRobin => {
                let node_id = self.next_round_robin_node()?;
                self.decision(actor_id, node_id, "round_robin", false)
            }
            VmPlacementPolicy::LeastConnections => {
                let node_id = self.least_connections_node()?;
                self.decision(actor_id, node_id, "least_connections", false)
            }
            VmPlacementPolicy::Pinned { node_id } => {
                if !self.active_nodes.contains_key(node_id) {
                    return Err(format!(
                        "error[vm_distributed_scheduler]: pinned node `{node_id}` is not active"
                    ));
                }
                self.decision(actor_id, node_id.clone(), "pinned", false)
            }
            VmPlacementPolicy::ShardAffinity {
                shard_key,
                fallback,
            } => self.place_shard_affinity(actor_id, shard_key, *fallback),
        }
    }

    /// Returns the known owner for a shard key, if one has been assigned.
    pub(crate) fn shard_owner(&self, shard_key: &str) -> Option<&str> {
        self.shard_owners.get(shard_key).map(String::as_str)
    }

    /// Returns the current placement assignment for one actor, if known.
    pub(crate) fn placement_assignment(&self, actor_id: &str) -> Option<&VmPlacementAssignment> {
        self.placement_assignments.get(actor_id)
    }

    /// Returns the number of active nodes available for placement.
    pub(crate) fn active_node_count(&self) -> usize {
        self.active_nodes.len()
    }

    /// Returns the active migration pressure limits for this scheduler.
    pub(crate) const fn limits(&self) -> VmSchedulingLimits {
        self.limits
    }

    /// Returns the number of in-flight migrations currently tracked.
    pub(crate) fn in_flight_migration_count(&self) -> usize {
        self.in_flight_migrations.len()
    }

    /// Requests a deterministic migration intent between two active nodes.
    pub(crate) fn request_migration(
        &mut self,
        actor_id: impl Into<String>,
        from_node_id: impl Into<String>,
        to_node_id: impl Into<String>,
        stateful: bool,
    ) -> Result<VmMigrationIntent, String> {
        self.request_migration_at_tick(actor_id, from_node_id, to_node_id, stateful, 0)
    }

    /// Requests a deterministic migration intent at a VM scheduler tick.
    pub(crate) fn request_migration_at_tick(
        &mut self,
        actor_id: impl Into<String>,
        from_node_id: impl Into<String>,
        to_node_id: impl Into<String>,
        stateful: bool,
        tick: u64,
    ) -> Result<VmMigrationIntent, String> {
        let actor_id = actor_id.into();
        let from_node_id = from_node_id.into();
        let to_node_id = to_node_id.into();
        self.validate_migration_pressure(tick)?;
        self.validate_actor_id(&actor_id)?;
        self.validate_active_node(&from_node_id, "source")?;
        self.validate_active_node(&to_node_id, "target")?;
        if from_node_id == to_node_id {
            return Err(
                "error[vm_distributed_scheduler]: migration source and target must differ"
                    .to_string(),
            );
        }
        if self.in_flight_migrations.contains_key(&actor_id) {
            return Err(format!(
                "error[vm_distributed_scheduler]: actor `{actor_id}` already has an in-flight migration"
            ));
        }
        self.next_migration_sequence += 1;
        *self.migrations_by_tick.entry(tick).or_insert(0) += 1;
        self.last_migration_tick = Some(tick);
        let intent = VmMigrationIntent {
            actor_id: actor_id.clone(),
            from_node_id,
            to_node_id,
            sequence: self.next_migration_sequence,
            stateful,
            phase: VmMigrationPhase::Requested,
            state_snapshot_ready: !stateful,
            in_flight_messages_ready: false,
        };
        self.in_flight_migrations.insert(actor_id, intent.clone());
        self.push_event(
            intent.actor_id.clone(),
            VmSchedulerEventKind::MigrationRequested {
                from_node_id: intent.from_node_id.clone(),
                to_node_id: intent.to_node_id.clone(),
                migration_sequence: intent.sequence,
                stateful: intent.stateful,
            },
        );
        Ok(intent)
    }

    /// Returns an in-flight migration intent for one actor.
    pub(crate) fn migration_intent(&self, actor_id: &str) -> Option<&VmMigrationIntent> {
        self.in_flight_migrations.get(actor_id)
    }

    /// Advances one in-flight migration through the next valid handoff phase.
    pub(crate) fn advance_migration(
        &mut self,
        actor_id: &str,
        sequence: u64,
        next_phase: VmMigrationPhase,
    ) -> Result<VmMigrationIntent, String> {
        self.validate_migration_sequence(actor_id, sequence)?;
        let intent = self
            .in_flight_migrations
            .get_mut(actor_id)
            .expect("validated migration exists");
        let expected_phase = next_migration_phase(intent.phase).ok_or_else(|| {
            format!(
                "error[vm_distributed_scheduler]: migration for actor `{actor_id}` cannot advance beyond `{:?}`",
                intent.phase
            )
        })?;
        if next_phase != expected_phase {
            return Err(format!(
                "error[vm_distributed_scheduler]: migration for actor `{actor_id}` expected next phase `{:?}` but got `{:?}`",
                expected_phase, next_phase
            ));
        }
        intent.phase = next_phase;
        if next_phase == VmMigrationPhase::Transferring {
            intent.state_snapshot_ready = true;
        }
        if next_phase == VmMigrationPhase::Resuming {
            intent.in_flight_messages_ready = true;
        }
        let updated = intent.clone();
        self.push_event(
            updated.actor_id.clone(),
            VmSchedulerEventKind::MigrationPhaseAdvanced {
                migration_sequence: updated.sequence,
                phase: updated.phase,
            },
        );
        Ok(updated)
    }

    /// Commits an in-flight migration intent with exact sequence matching.
    pub(crate) fn commit_migration(
        &mut self,
        actor_id: &str,
        sequence: u64,
    ) -> Result<VmMigrationOutcome, String> {
        let expected = VmMigrationOutcome::Committed { sequence };
        if let Some(outcome) = self.replay_terminal_outcome(actor_id, sequence, &expected)? {
            return Ok(outcome);
        }
        let intent = self.validate_migration_sequence(actor_id, sequence)?;
        if intent.phase != VmMigrationPhase::Resuming {
            return Err(format!(
                "error[vm_distributed_scheduler]: migration for actor `{actor_id}` must reach `Resuming` before commit"
            ));
        }
        if !intent.state_snapshot_ready || !intent.in_flight_messages_ready {
            return Err(format!(
                "error[vm_distributed_scheduler]: migration for actor `{actor_id}` cannot commit before state snapshot and in-flight message contracts are satisfied"
            ));
        }
        let outcome = self.finish_migration(actor_id, sequence, |sequence| {
            VmMigrationOutcome::Committed { sequence }
        })?;
        self.push_event(
            actor_id.to_string(),
            VmSchedulerEventKind::MigrationCommitted {
                migration_sequence: sequence,
            },
        );
        Ok(outcome)
    }

    /// Rolls back an in-flight migration intent with a typed reason.
    pub(crate) fn rollback_migration(
        &mut self,
        actor_id: &str,
        sequence: u64,
        reason: impl Into<String>,
    ) -> Result<VmMigrationOutcome, String> {
        let reason = self.validate_reason(reason)?;
        let expected = VmMigrationOutcome::RolledBack {
            sequence,
            reason: reason.clone(),
        };
        if let Some(outcome) = self.replay_terminal_outcome(actor_id, sequence, &expected)? {
            return Ok(outcome);
        }
        let event_reason = reason.clone();
        let outcome = self.finish_migration(actor_id, sequence, |sequence| {
            VmMigrationOutcome::RolledBack { sequence, reason }
        })?;
        self.push_event(
            actor_id.to_string(),
            VmSchedulerEventKind::MigrationRolledBack {
                migration_sequence: sequence,
                reason: event_reason,
            },
        );
        Ok(outcome)
    }

    /// Aborts an in-flight migration intent with a typed reason.
    pub(crate) fn abort_migration(
        &mut self,
        actor_id: &str,
        sequence: u64,
        reason: impl Into<String>,
    ) -> Result<VmMigrationOutcome, String> {
        let reason = self.validate_reason(reason)?;
        let expected = VmMigrationOutcome::Aborted {
            sequence,
            reason: reason.clone(),
        };
        if let Some(outcome) = self.replay_terminal_outcome(actor_id, sequence, &expected)? {
            return Ok(outcome);
        }
        let event_reason = reason.clone();
        let outcome = self.finish_migration(actor_id, sequence, |sequence| {
            VmMigrationOutcome::Aborted { sequence, reason }
        })?;
        self.push_event(
            actor_id.to_string(),
            VmSchedulerEventKind::MigrationAborted {
                migration_sequence: sequence,
                reason: event_reason,
            },
        );
        Ok(outcome)
    }

    /// Returns the full scheduler event log in deterministic order.
    pub(crate) fn events(&self) -> &[VmSchedulerEvent] {
        &self.events
    }

    /// Returns scheduler events after an event sequence cursor.
    pub(crate) fn events_after(&self, event_sequence: u64) -> Vec<VmSchedulerEvent> {
        self.events
            .iter()
            .filter(|event| event.event_sequence > event_sequence)
            .cloned()
            .collect()
    }

    /// Applies a placement update from a replayed scheduler event.
    pub(crate) fn apply_placement_update(
        &mut self,
        event: VmSchedulerEvent,
    ) -> Result<VmPlacementAssignment, String> {
        if event.event_sequence == 0 {
            return Err(
                "error[vm_distributed_scheduler]: placement update sequence must be non-zero"
                    .to_string(),
            );
        }
        self.validate_actor_id(&event.actor_id)?;
        let VmSchedulerEventKind::Placement {
            node_id,
            policy,
            fallback_used,
        } = event.kind
        else {
            return Err(format!(
                "error[vm_distributed_scheduler]: event sequence `{}` is not a placement update",
                event.event_sequence
            ));
        };
        if !self.active_nodes.contains_key(&node_id) {
            return Err(format!(
                "error[vm_distributed_scheduler]: placement update node `{node_id}` is not active"
            ));
        }
        let decision = VmPlacementDecision {
            actor_id: event.actor_id,
            node_id,
            policy,
            fallback_used,
        };
        let incoming = VmPlacementAssignment {
            decision,
            event_sequence: event.event_sequence,
        };
        match self
            .placement_assignments
            .get(&incoming.decision.actor_id)
        {
            Some(existing) if incoming.event_sequence < existing.event_sequence => {
                let reason = format!(
                    "error[vm_distributed_scheduler]: stale placement update sequence `{}` is older than current sequence `{}` for actor `{}`",
                    incoming.event_sequence, existing.event_sequence, incoming.decision.actor_id
                );
                let envelope = VmDistributedFailureEnvelope {
                    node_id: incoming.decision.node_id.clone(),
                    tick: incoming.event_sequence,
                    kind: VmDistributedFailureKind::StalePlacementUpdate {
                        actor_id: incoming.decision.actor_id.clone(),
                        incoming_sequence: incoming.event_sequence,
                        current_sequence: existing.event_sequence,
                    },
                    reason: reason.clone(),
                };
                if !self.failure_envelopes.contains(&envelope) {
                    self.failure_envelopes.push(envelope);
                }
                Err(reason)
            }
            Some(existing)
                if incoming.event_sequence == existing.event_sequence && &incoming != existing =>
            {
                Err(format!(
                    "error[vm_distributed_scheduler]: conflicting placement update sequence `{}` for actor `{}`",
                    incoming.event_sequence, incoming.decision.actor_id
                ))
            }
            Some(existing) if incoming.event_sequence == existing.event_sequence => {
                Ok(existing.clone())
            }
            _ => {
                self.placement_assignments
                    .insert(incoming.decision.actor_id.clone(), incoming.clone());
                Ok(incoming)
            }
        }
    }

    /// Applies shard-affinity placement with an explicit fallback policy.
    fn place_shard_affinity(
        &mut self,
        actor_id: String,
        shard_key: &str,
        fallback: VmPlacementFallback,
    ) -> Result<VmPlacementDecision, String> {
        if shard_key.is_empty() {
            return Err("error[vm_distributed_scheduler]: shard key must be non-empty".to_string());
        }
        if let Some(node_id) = self.shard_owners.get(shard_key) {
            if self.active_nodes.contains_key(node_id) {
                return self.decision(actor_id, node_id.clone(), "shard_affinity", false);
            }
            if fallback == VmPlacementFallback::Reject {
                return Err(format!(
                    "error[vm_distributed_scheduler]: shard `{shard_key}` owner `{node_id}` is not active"
                ));
            }
        }
        let node_id = self.next_round_robin_node()?;
        self.shard_owners
            .insert(shard_key.to_string(), node_id.clone());
        self.decision(actor_id, node_id, "shard_affinity", true)
    }

    /// Selects the next active node in deterministic round-robin order.
    fn next_round_robin_node(&mut self) -> Result<String, String> {
        let nodes = self.active_node_ids();
        if nodes.is_empty() {
            return Err("error[vm_distributed_scheduler]: no active nodes available".to_string());
        }
        let node_id = nodes[self.round_robin_cursor % nodes.len()].clone();
        self.round_robin_cursor = (self.round_robin_cursor + 1) % nodes.len();
        Ok(node_id)
    }

    /// Selects the active node with the fewest connections and stable tie-breaks.
    fn least_connections_node(&self) -> Result<String, String> {
        self.active_nodes
            .values()
            .min_by_key(|load| (load.active_connections, load.node_id.as_str()))
            .map(|load| load.node_id.clone())
            .ok_or_else(|| "error[vm_distributed_scheduler]: no active nodes available".to_string())
    }

    /// Returns active node ids in deterministic order.
    fn active_node_ids(&self) -> Vec<String> {
        self.active_nodes.keys().cloned().collect()
    }

    /// Builds a placement decision with common metadata.
    fn decision(
        &mut self,
        actor_id: String,
        node_id: String,
        policy: &'static str,
        fallback_used: bool,
    ) -> Result<VmPlacementDecision, String> {
        let decision = VmPlacementDecision {
            actor_id,
            node_id,
            policy,
            fallback_used,
        };
        let event_sequence = self.push_event(
            decision.actor_id.clone(),
            VmSchedulerEventKind::Placement {
                node_id: decision.node_id.clone(),
                policy: decision.policy,
                fallback_used: decision.fallback_used,
            },
        );
        self.placement_assignments.insert(
            decision.actor_id.clone(),
            VmPlacementAssignment {
                decision: decision.clone(),
                event_sequence,
            },
        );
        Ok(decision)
    }

    /// Finishes a migration only when the actor and sequence match exactly.
    fn finish_migration(
        &mut self,
        actor_id: &str,
        sequence: u64,
        outcome: impl FnOnce(u64) -> VmMigrationOutcome,
    ) -> Result<VmMigrationOutcome, String> {
        self.validate_migration_sequence(actor_id, sequence)?;
        self.in_flight_migrations.remove(actor_id);
        let outcome = outcome(sequence);
        self.completed_migration_outcomes
            .insert((actor_id.to_string(), sequence), outcome.clone());
        Ok(outcome)
    }

    /// Replays a completed terminal migration when the retried outcome matches.
    fn replay_terminal_outcome(
        &self,
        actor_id: &str,
        sequence: u64,
        expected: &VmMigrationOutcome,
    ) -> Result<Option<VmMigrationOutcome>, String> {
        match self
            .completed_migration_outcomes
            .get(&(actor_id.to_string(), sequence))
        {
            Some(existing) if existing == expected => Ok(Some(existing.clone())),
            Some(existing) => Err(format!(
                "error[vm_distributed_scheduler]: migration for actor `{actor_id}` sequence `{sequence}` already completed with incompatible outcome `{:?}`",
                existing
            )),
            None => Ok(None),
        }
    }

    /// Validates that a migration exists and the outcome sequence is current.
    fn validate_migration_sequence(
        &self,
        actor_id: &str,
        sequence: u64,
    ) -> Result<&VmMigrationIntent, String> {
        let intent = self.in_flight_migrations.get(actor_id).ok_or_else(|| {
            format!(
                "error[vm_distributed_scheduler]: actor `{actor_id}` has no in-flight migration"
            )
        })?;
        if intent.sequence != sequence {
            return Err(format!(
                "error[vm_distributed_scheduler]: migration outcome sequence `{sequence}` does not match expected `{}` for actor `{actor_id}`",
                intent.sequence
            ));
        }
        Ok(intent)
    }

    /// Validates that an actor id can be used for placement or migration.
    fn validate_actor_id(&self, actor_id: &str) -> Result<(), String> {
        if actor_id.is_empty() {
            return Err("error[vm_distributed_scheduler]: actor id must be non-empty".to_string());
        }
        Ok(())
    }

    /// Validates that a node participates in the current active scheduler view.
    fn validate_active_node(&self, node_id: &str, role: &str) -> Result<(), String> {
        if self.active_nodes.contains_key(node_id) {
            return Ok(());
        }
        Err(format!(
            "error[vm_distributed_scheduler]: migration {role} node `{node_id}` is not active"
        ))
    }

    /// Validates a human-readable terminal migration reason.
    fn validate_reason(&self, reason: impl Into<String>) -> Result<String, String> {
        let reason = reason.into();
        if reason.is_empty() {
            return Err(
                "error[vm_distributed_scheduler]: migration outcome reason must be non-empty"
                    .to_string(),
            );
        }
        Ok(reason)
    }

    /// Validates scheduler-level migration backpressure for one request tick.
    fn validate_migration_pressure(&self, tick: u64) -> Result<(), String> {
        if self.in_flight_migrations.len() >= self.limits.max_in_flight_migrations {
            return Err(format!(
                "error[vm_distributed_scheduler]: in-flight migration limit `{}` reached",
                self.limits.max_in_flight_migrations
            ));
        }
        if let Some(last_tick) = self.last_migration_tick {
            if tick < last_tick {
                return Err(format!(
                    "error[vm_distributed_scheduler]: migration tick `{tick}` is older than last migration tick `{last_tick}`"
                ));
            }
            if tick.saturating_sub(last_tick) < self.limits.min_migration_interval_ticks {
                return Err(format!(
                    "error[vm_distributed_scheduler]: migration tick `{tick}` violates minimum interval `{}` after tick `{last_tick}`",
                    self.limits.min_migration_interval_ticks
                ));
            }
        }
        let current_tick_count = self.migrations_by_tick.get(&tick).copied().unwrap_or(0);
        if current_tick_count >= self.limits.max_migrations_per_tick {
            return Err(format!(
                "error[vm_distributed_scheduler]: migration tick `{tick}` reached per-tick limit `{}`",
                self.limits.max_migrations_per_tick
            ));
        }
        Ok(())
    }

    /// Appends a typed scheduler event with a monotonic event sequence.
    fn push_event(&mut self, actor_id: String, kind: VmSchedulerEventKind) -> u64 {
        self.next_event_sequence += 1;
        self.events.push(VmSchedulerEvent {
            event_sequence: self.next_event_sequence,
            actor_id,
            kind,
        });
        self.next_event_sequence
    }
}

/// Validates scheduler limits before construction.
fn validate_scheduling_limits(limits: VmSchedulingLimits) -> Result<(), String> {
    if limits.max_in_flight_migrations == 0 {
        return Err(
            "error[vm_distributed_scheduler]: max in-flight migrations must be non-zero"
                .to_string(),
        );
    }
    if limits.max_migrations_per_tick == 0 {
        return Err(
            "error[vm_distributed_scheduler]: max migrations per tick must be non-zero".to_string(),
        );
    }
    Ok(())
}

/// Returns the only valid next migration phase.
fn next_migration_phase(phase: VmMigrationPhase) -> Option<VmMigrationPhase> {
    match phase {
        VmMigrationPhase::Requested => Some(VmMigrationPhase::Snapshotting),
        VmMigrationPhase::Snapshotting => Some(VmMigrationPhase::Transferring),
        VmMigrationPhase::Transferring => Some(VmMigrationPhase::Resuming),
        VmMigrationPhase::Resuming => None,
    }
}

#[cfg(test)]
#[path = "distributed_scheduler_test.rs"]
mod distributed_scheduler_test;
