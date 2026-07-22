use super::{VmDistributedScheduler, VmMigrationOutcome, VmMigrationPhase, VmSchedulerEventKind};

/// Distributed fault state for one VM cluster node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedFaultState {
    Recovered,
    Suspected,
    Degraded,
    Isolated,
    Recovering,
}

/// VM-owned threshold policy for heartbeat-driven fault detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedFaultPolicy {
    pub(crate) suspicion_threshold_ticks: u64,
    pub(crate) isolation_threshold_ticks: u64,
    pub(crate) recovery_window_ticks: u64,
}

impl VmDistributedFaultPolicy {
    /// Builds a fault policy with explicit non-zero ordered thresholds.
    pub(crate) fn new(
        suspicion_threshold_ticks: u64,
        isolation_threshold_ticks: u64,
        recovery_window_ticks: u64,
    ) -> Result<Self, String> {
        let policy = Self {
            suspicion_threshold_ticks,
            isolation_threshold_ticks,
            recovery_window_ticks,
        };
        validate_fault_policy(policy)?;
        Ok(policy)
    }

    /// Resolves mismatched peers to the stricter deterministic thresholds.
    pub(crate) fn resolve(self, peer: Self) -> Self {
        Self {
            suspicion_threshold_ticks: self
                .suspicion_threshold_ticks
                .max(peer.suspicion_threshold_ticks),
            isolation_threshold_ticks: self
                .isolation_threshold_ticks
                .max(peer.isolation_threshold_ticks),
            recovery_window_ticks: self.recovery_window_ticks.max(peer.recovery_window_ticks),
        }
    }
}

/// Explicit compatibility outcome for nodes without partition-tolerant execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedCompatibilityOutcome {
    Supported,
    FallbackLocalOnly,
    FeatureUnsupported,
}

impl VmDistributedCompatibilityOutcome {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::FallbackLocalOnly => "fallback_local_only",
            Self::FeatureUnsupported => "feature_unsupported",
        }
    }
}

/// Classifies support without silently attempting unsafe distributed execution.
pub(crate) const fn distributed_fault_compatibility(
    partition_tolerant: bool,
    local_fallback_available: bool,
) -> VmDistributedCompatibilityOutcome {
    if partition_tolerant {
        VmDistributedCompatibilityOutcome::Supported
    } else if local_fallback_available {
        VmDistributedCompatibilityOutcome::FallbackLocalOnly
    } else {
        VmDistributedCompatibilityOutcome::FeatureUnsupported
    }
}

impl Default for VmDistributedFaultPolicy {
    /// Returns conservative local defaults for VM scheduler tests.
    fn default() -> Self {
        Self {
            suspicion_threshold_ticks: 3,
            isolation_threshold_ticks: 6,
            recovery_window_ticks: 12,
        }
    }
}

/// Result of recording a heartbeat in the distributed fault scheduler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedHeartbeatObservation {
    Recorded { node_id: String, tick: u64 },
    DuplicateSuppressed { node_id: String, tick: u64 },
}

/// Replayable diagnostic for one distributed fault-state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedFaultTransition {
    pub(crate) node_id: String,
    pub(crate) previous_state: VmDistributedFaultState,
    pub(crate) next_state: VmDistributedFaultState,
    pub(crate) tick: u64,
    pub(crate) reason: String,
}

/// Current fault-tracking status for one known active node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VmDistributedFaultStatus {
    pub(super) state: VmDistributedFaultState,
    pub(super) last_tick: u64,
}

impl VmDistributedFaultStatus {
    /// Returns a recovered status for newly active nodes.
    pub(super) const fn recovered() -> Self {
        Self {
            state: VmDistributedFaultState::Recovered,
            last_tick: 0,
        }
    }
}

/// Typed distributed failure category emitted by VM-owned recovery checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedFailureKind {
    HeartbeatMissed {
        node_id: String,
        last_heartbeat_tick: u64,
        current_tick: u64,
    },
    PartitionSuspected {
        node_id: String,
        last_heartbeat_tick: u64,
        current_tick: u64,
        gap_ticks: u64,
    },
    RecoveryWindowExpired {
        node_id: String,
        recovery_started_tick: u64,
        current_tick: u64,
        window_ticks: u64,
    },
    MigrationTimeout {
        actor_id: String,
        migration_sequence: u64,
        phase: VmMigrationPhase,
    },
    MigrationPartialCommit {
        actor_id: String,
        migration_sequence: u64,
        phase: VmMigrationPhase,
    },
    StalePlacementUpdate {
        actor_id: String,
        incoming_sequence: u64,
        current_sequence: u64,
    },
}

impl VmDistributedFailureKind {
    /// Stable machine-readable failure category.
    pub(crate) const fn label(&self) -> &'static str {
        match self {
            Self::HeartbeatMissed { .. } => "heartbeat_missed",
            Self::PartitionSuspected { .. } => "partition_suspected",
            Self::RecoveryWindowExpired { .. } => "recovery_window_expired",
            Self::MigrationTimeout { .. } => "migration_timeout",
            Self::MigrationPartialCommit { .. } => "migration_partial_commit",
            Self::StalePlacementUpdate { .. } => "stale_placement_update",
        }
    }

    /// Stable machine-readable operational decision represented by the failure.
    pub(crate) const fn diagnostic_kind(&self) -> &'static str {
        match self {
            Self::HeartbeatMissed { .. } => "partition_onset",
            Self::PartitionSuspected { .. } => "node_role_demotion",
            Self::RecoveryWindowExpired { .. } => "recovery_window_expiry",
            Self::MigrationTimeout { .. } | Self::MigrationPartialCommit { .. } => {
                "migration_rollback_decision"
            }
            Self::StalePlacementUpdate { .. } => "stale_placement_rejection",
        }
    }
}

impl VmDistributedFaultTransition {
    /// Stable machine-readable decision represented by the state transition.
    pub(crate) const fn diagnostic_kind(&self) -> &'static str {
        match self.next_state {
            VmDistributedFaultState::Suspected => "partition_onset",
            VmDistributedFaultState::Degraded => "suspect_quorum",
            VmDistributedFaultState::Isolated => "node_role_demotion",
            VmDistributedFaultState::Recovering => "recovery_started",
            VmDistributedFaultState::Recovered => "recovery_completion",
        }
    }
}

/// Replayable distributed failure envelope for diagnostics and recovery logic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedFailureEnvelope {
    pub(crate) node_id: String,
    pub(crate) tick: u64,
    pub(crate) kind: VmDistributedFailureKind,
    pub(crate) reason: String,
}

impl VmDistributedScheduler {
    /// Records a heartbeat tick for fault detection and suppresses duplicates.
    pub(crate) fn record_fault_heartbeat_at_tick(
        &mut self,
        node_id: &str,
        tick: u64,
    ) -> Result<VmDistributedHeartbeatObservation, String> {
        self.validate_active_fault_node(node_id)?;
        let last_tick = self.last_heartbeat_tick(node_id)?;
        if tick < last_tick {
            return Err(format!(
                "error[vm_distributed_scheduler]: stale heartbeat tick `{tick}` is older than last heartbeat tick `{last_tick}` for node `{node_id}`"
            ));
        }
        if tick == last_tick {
            return Ok(VmDistributedHeartbeatObservation::DuplicateSuppressed {
                node_id: node_id.to_string(),
                tick,
            });
        }
        self.last_heartbeat_ticks.insert(node_id.to_string(), tick);
        Ok(VmDistributedHeartbeatObservation::Recorded {
            node_id: node_id.to_string(),
            tick,
        })
    }

    /// Marks a node suspected when its heartbeat gap exceeds the policy threshold.
    pub(crate) fn suspect_missed_heartbeat_at_tick(
        &mut self,
        node_id: &str,
        current_tick: u64,
        reason: impl Into<String>,
    ) -> Result<Option<VmDistributedFaultTransition>, String> {
        let reason = self.validate_fault_reason(reason)?;
        self.validate_active_fault_node(node_id)?;
        if let Some(transition) = self.replay_fault_transition(
            node_id,
            VmDistributedFaultState::Suspected,
            current_tick,
            &reason,
        )? {
            return Ok(Some(transition));
        }
        let last_heartbeat_tick = self.last_heartbeat_tick(node_id)?;
        if current_tick < last_heartbeat_tick {
            return Err(format!(
                "error[vm_distributed_scheduler]: heartbeat evaluation tick `{current_tick}` is older than last heartbeat tick `{last_heartbeat_tick}` for node `{node_id}`"
            ));
        }
        let gap = current_tick.saturating_sub(last_heartbeat_tick);
        if gap <= self.fault_policy.suspicion_threshold_ticks {
            return Ok(None);
        }
        if self.fault_state(node_id) != Some(VmDistributedFaultState::Recovered) {
            return Ok(None);
        }
        let transition = self.transition_fault_state_at_tick(
            node_id,
            VmDistributedFaultState::Suspected,
            current_tick,
            reason.clone(),
        )?;
        self.failure_envelopes.push(VmDistributedFailureEnvelope {
            node_id: node_id.to_string(),
            tick: current_tick,
            kind: VmDistributedFailureKind::HeartbeatMissed {
                node_id: node_id.to_string(),
                last_heartbeat_tick,
                current_tick,
            },
            reason,
        });
        Ok(Some(transition))
    }

    /// Isolates a suspected node when its heartbeat gap exceeds the policy threshold.
    pub(crate) fn isolate_missed_heartbeat_at_tick(
        &mut self,
        node_id: &str,
        current_tick: u64,
        reason: impl Into<String>,
    ) -> Result<Option<VmDistributedFaultTransition>, String> {
        let reason = self.validate_fault_reason(reason)?;
        self.validate_active_fault_node(node_id)?;
        if let Some(transition) = self.replay_fault_transition(
            node_id,
            VmDistributedFaultState::Isolated,
            current_tick,
            &reason,
        )? {
            return Ok(Some(transition));
        }
        let last_heartbeat_tick = self.last_heartbeat_tick(node_id)?;
        if current_tick < last_heartbeat_tick {
            return Err(format!(
                "error[vm_distributed_scheduler]: heartbeat isolation tick `{current_tick}` is older than last heartbeat tick `{last_heartbeat_tick}` for node `{node_id}`"
            ));
        }
        let gap = current_tick.saturating_sub(last_heartbeat_tick);
        if gap <= self.fault_policy.isolation_threshold_ticks {
            return Ok(None);
        }
        if !matches!(
            self.fault_state(node_id),
            Some(VmDistributedFaultState::Suspected | VmDistributedFaultState::Degraded)
        ) {
            return Err(format!(
                "error[vm_distributed_scheduler]: node `{node_id}` must be `Suspected` or `Degraded` before isolation"
            ));
        }
        let transition = self.transition_fault_state_at_tick(
            node_id,
            VmDistributedFaultState::Isolated,
            current_tick,
            reason.clone(),
        )?;
        self.failure_envelopes.push(VmDistributedFailureEnvelope {
            node_id: node_id.to_string(),
            tick: current_tick,
            kind: VmDistributedFailureKind::PartitionSuspected {
                node_id: node_id.to_string(),
                last_heartbeat_tick,
                current_tick,
                gap_ticks: gap,
            },
            reason,
        });
        Ok(Some(transition))
    }

    /// Re-isolates a recovering node when its recovery window expires.
    pub(crate) fn expire_recovery_window_at_tick(
        &mut self,
        node_id: &str,
        current_tick: u64,
        reason: impl Into<String>,
    ) -> Result<Option<VmDistributedFaultTransition>, String> {
        let reason = self.validate_fault_reason(reason)?;
        self.validate_active_fault_node(node_id)?;
        if let Some(transition) = self.replay_fault_transition(
            node_id,
            VmDistributedFaultState::Isolated,
            current_tick,
            &reason,
        )? {
            return Ok(Some(transition));
        }
        let Some(status) = self.fault_states.get(node_id) else {
            return Err(format!(
                "error[vm_distributed_scheduler]: missing fault state for node `{node_id}`"
            ));
        };
        if status.state != VmDistributedFaultState::Recovering {
            return Err(format!(
                "error[vm_distributed_scheduler]: node `{node_id}` must be `Recovering` before recovery window expiry"
            ));
        }
        if current_tick < status.last_tick {
            return Err(format!(
                "error[vm_distributed_scheduler]: recovery expiry tick `{current_tick}` is older than recovery start tick `{}` for node `{node_id}`",
                status.last_tick
            ));
        }
        let elapsed = current_tick.saturating_sub(status.last_tick);
        if elapsed <= self.fault_policy.recovery_window_ticks {
            return Ok(None);
        }
        let recovery_started_tick = status.last_tick;
        let window_ticks = self.fault_policy.recovery_window_ticks;
        let transition = self.transition_fault_state_at_tick(
            node_id,
            VmDistributedFaultState::Isolated,
            current_tick,
            reason.clone(),
        )?;
        self.failure_envelopes.push(VmDistributedFailureEnvelope {
            node_id: node_id.to_string(),
            tick: current_tick,
            kind: VmDistributedFailureKind::RecoveryWindowExpired {
                node_id: node_id.to_string(),
                recovery_started_tick,
                current_tick,
                window_ticks,
            },
            reason,
        });
        Ok(Some(transition))
    }

    /// Completes recovery for a recovering node and refreshes its heartbeat tick.
    pub(crate) fn complete_recovery_at_tick(
        &mut self,
        node_id: &str,
        current_tick: u64,
        reason: impl Into<String>,
    ) -> Result<VmDistributedFaultTransition, String> {
        let reason = self.validate_fault_reason(reason)?;
        self.validate_active_fault_node(node_id)?;
        if let Some(transition) = self.replay_fault_transition(
            node_id,
            VmDistributedFaultState::Recovered,
            current_tick,
            &reason,
        )? {
            return Ok(transition);
        }
        let Some(status) = self.fault_states.get(node_id) else {
            return Err(format!(
                "error[vm_distributed_scheduler]: missing fault state for node `{node_id}`"
            ));
        };
        if status.state != VmDistributedFaultState::Recovering {
            return Err(format!(
                "error[vm_distributed_scheduler]: node `{node_id}` must be `Recovering` before recovery completion"
            ));
        }
        if current_tick < status.last_tick {
            return Err(format!(
                "error[vm_distributed_scheduler]: recovery completion tick `{current_tick}` is older than recovery start tick `{}` for node `{node_id}`",
                status.last_tick
            ));
        }
        let transition = self.transition_fault_state_at_tick(
            node_id,
            VmDistributedFaultState::Recovered,
            current_tick,
            reason,
        )?;
        self.last_heartbeat_ticks
            .entry(node_id.to_string())
            .and_modify(|tick| *tick = (*tick).max(current_tick))
            .or_insert(current_tick);
        Ok(transition)
    }

    /// Rolls back an in-flight migration due to a VM-observed timeout.
    pub(crate) fn timeout_migration_at_tick(
        &mut self,
        actor_id: &str,
        sequence: u64,
        tick: u64,
        reason: impl Into<String>,
    ) -> Result<VmMigrationOutcome, String> {
        let reason = self.validate_failure_reason(reason)?;
        let expected = VmMigrationOutcome::RolledBack {
            sequence,
            reason: reason.clone(),
        };
        if let Some(outcome) = self.replay_terminal_outcome(actor_id, sequence, &expected)? {
            return Ok(outcome);
        }
        let intent = self
            .validate_migration_sequence(actor_id, sequence)?
            .clone();
        let outcome_reason = reason.clone();
        let outcome = self.finish_migration(actor_id, sequence, |sequence| {
            VmMigrationOutcome::RolledBack {
                sequence,
                reason: outcome_reason,
            }
        })?;
        self.failure_envelopes.push(VmDistributedFailureEnvelope {
            node_id: intent.to_node_id,
            tick,
            kind: VmDistributedFailureKind::MigrationTimeout {
                actor_id: actor_id.to_string(),
                migration_sequence: sequence,
                phase: intent.phase,
            },
            reason: reason.clone(),
        });
        self.push_event(
            actor_id.to_string(),
            VmSchedulerEventKind::MigrationRolledBack {
                migration_sequence: sequence,
                reason,
            },
        );
        Ok(outcome)
    }

    /// Rolls back a migration when the VM observes an unsafe partial commit.
    pub(crate) fn partial_commit_migration_at_tick(
        &mut self,
        actor_id: &str,
        sequence: u64,
        tick: u64,
        reason: impl Into<String>,
    ) -> Result<VmMigrationOutcome, String> {
        let reason = self.validate_failure_reason(reason)?;
        let expected = VmMigrationOutcome::RolledBack {
            sequence,
            reason: reason.clone(),
        };
        if let Some(outcome) = self.replay_terminal_outcome(actor_id, sequence, &expected)? {
            return Ok(outcome);
        }
        let intent = self
            .validate_migration_sequence(actor_id, sequence)?
            .clone();
        if intent.phase == VmMigrationPhase::Resuming {
            return Err(format!(
                "error[vm_distributed_scheduler]: migration for actor `{actor_id}` sequence `{sequence}` already reached `Resuming` and cannot be marked as a partial commit"
            ));
        }
        let outcome_reason = reason.clone();
        let outcome = self.finish_migration(actor_id, sequence, |sequence| {
            VmMigrationOutcome::RolledBack {
                sequence,
                reason: outcome_reason,
            }
        })?;
        self.failure_envelopes.push(VmDistributedFailureEnvelope {
            node_id: intent.to_node_id,
            tick,
            kind: VmDistributedFailureKind::MigrationPartialCommit {
                actor_id: actor_id.to_string(),
                migration_sequence: sequence,
                phase: intent.phase,
            },
            reason: reason.clone(),
        });
        self.push_event(
            actor_id.to_string(),
            VmSchedulerEventKind::MigrationRolledBack {
                migration_sequence: sequence,
                reason,
            },
        );
        Ok(outcome)
    }

    /// Returns the tracked fault state for one active node.
    pub(crate) fn fault_state(&self, node_id: &str) -> Option<VmDistributedFaultState> {
        self.fault_states.get(node_id).map(|status| status.state)
    }

    /// Returns the scheduler's current heartbeat fault-detection policy.
    pub(crate) const fn fault_policy(&self) -> VmDistributedFaultPolicy {
        self.fault_policy
    }

    /// Returns replayable fault diagnostics after a scheduler tick cursor.
    pub(crate) fn fault_transitions_after(&self, tick: u64) -> Vec<VmDistributedFaultTransition> {
        let mut transitions = self
            .fault_transitions
            .iter()
            .filter(|transition| transition.tick > tick)
            .cloned()
            .collect::<Vec<_>>();
        transitions.sort_by(|left, right| {
            (
                left.tick,
                left.node_id.as_str(),
                left.diagnostic_kind(),
                left.reason.as_str(),
            )
                .cmp(&(
                    right.tick,
                    right.node_id.as_str(),
                    right.diagnostic_kind(),
                    right.reason.as_str(),
                ))
        });
        transitions
    }

    /// Returns replayable distributed failure envelopes after a tick cursor.
    pub(crate) fn failure_envelopes_after(&self, tick: u64) -> Vec<VmDistributedFailureEnvelope> {
        let mut envelopes = self
            .failure_envelopes
            .iter()
            .filter(|envelope| envelope.tick > tick)
            .cloned()
            .collect::<Vec<_>>();
        envelopes.sort_by(|left, right| {
            (
                left.tick,
                left.node_id.as_str(),
                left.kind.label(),
                left.reason.as_str(),
            )
                .cmp(&(
                    right.tick,
                    right.node_id.as_str(),
                    right.kind.label(),
                    right.reason.as_str(),
                ))
        });
        envelopes
    }

    /// Applies a legal distributed fault-state transition for one active node.
    pub(crate) fn transition_fault_state_at_tick(
        &mut self,
        node_id: impl Into<String>,
        next_state: VmDistributedFaultState,
        tick: u64,
        reason: impl Into<String>,
    ) -> Result<VmDistributedFaultTransition, String> {
        let node_id = node_id.into();
        let reason = self.validate_fault_reason(reason)?;
        self.validate_active_fault_node(&node_id)?;
        if let Some(transition) =
            self.replay_fault_transition(&node_id, next_state, tick, &reason)?
        {
            return Ok(transition);
        }
        let status = self
            .fault_states
            .get_mut(&node_id)
            .expect("validated active node has fault status");
        if tick < status.last_tick {
            return Err(format!(
                "error[vm_distributed_scheduler]: fault tick `{tick}` is older than last fault tick `{}` for node `{node_id}`",
                status.last_tick
            ));
        }
        if !is_valid_fault_transition(status.state, next_state) {
            return Err(format!(
                "error[vm_distributed_scheduler]: invalid fault transition for node `{node_id}` from `{:?}` to `{:?}`",
                status.state, next_state
            ));
        }
        let transition = VmDistributedFaultTransition {
            node_id,
            previous_state: status.state,
            next_state,
            tick,
            reason,
        };
        status.state = next_state;
        status.last_tick = tick;
        self.fault_transitions.push(transition.clone());
        Ok(transition)
    }

    /// Replays a previously recorded fault transition when retry metadata matches.
    fn replay_fault_transition(
        &self,
        node_id: &str,
        next_state: VmDistributedFaultState,
        tick: u64,
        reason: &str,
    ) -> Result<Option<VmDistributedFaultTransition>, String> {
        let Some(existing) = self
            .fault_transitions
            .iter()
            .find(|transition| transition.node_id == node_id && transition.tick == tick)
        else {
            return Ok(None);
        };
        if existing.next_state == next_state && existing.reason == reason {
            return Ok(Some(existing.clone()));
        }
        Err(format!(
            "error[vm_distributed_scheduler]: fault transition for node `{node_id}` tick `{tick}` already recorded as `{:?}` with reason `{}`",
            existing.next_state, existing.reason
        ))
    }

    /// Validates that a node participates in fault tracking.
    fn validate_active_fault_node(&self, node_id: &str) -> Result<(), String> {
        if self.active_nodes.contains_key(node_id) {
            return Ok(());
        }
        Err(format!(
            "error[vm_distributed_scheduler]: fault node `{node_id}` is not active"
        ))
    }

    /// Validates a human-readable distributed fault transition reason.
    fn validate_fault_reason(&self, reason: impl Into<String>) -> Result<String, String> {
        let reason = reason.into();
        if reason.is_empty() {
            return Err(
                "error[vm_distributed_scheduler]: fault transition reason must be non-empty"
                    .to_string(),
            );
        }
        Ok(reason)
    }

    /// Validates a human-readable distributed failure reason.
    fn validate_failure_reason(&self, reason: impl Into<String>) -> Result<String, String> {
        let reason = reason.into();
        if reason.is_empty() {
            return Err(
                "error[vm_distributed_scheduler]: failure envelope reason must be non-empty"
                    .to_string(),
            );
        }
        Ok(reason)
    }

    /// Returns the last recorded heartbeat tick for one active node.
    fn last_heartbeat_tick(&self, node_id: &str) -> Result<u64, String> {
        self.last_heartbeat_ticks
            .get(node_id)
            .copied()
            .ok_or_else(|| {
                format!(
                    "error[vm_distributed_scheduler]: missing heartbeat state for node `{node_id}`"
                )
            })
    }
}

/// Validates heartbeat-driven distributed fault policy thresholds.
pub(super) fn validate_fault_policy(policy: VmDistributedFaultPolicy) -> Result<(), String> {
    if policy.suspicion_threshold_ticks == 0 {
        return Err(
            "error[vm_distributed_scheduler]: suspicion threshold ticks must be non-zero"
                .to_string(),
        );
    }
    if policy.isolation_threshold_ticks <= policy.suspicion_threshold_ticks {
        return Err(format!(
            "error[vm_distributed_scheduler]: isolation threshold `{}` must be greater than suspicion threshold `{}`",
            policy.isolation_threshold_ticks, policy.suspicion_threshold_ticks
        ));
    }
    if policy.recovery_window_ticks == 0 {
        return Err(
            "error[vm_distributed_scheduler]: recovery window ticks must be non-zero".to_string(),
        );
    }
    if policy.recovery_window_ticks <= policy.isolation_threshold_ticks {
        return Err(format!(
            "error[vm_distributed_scheduler]: recovery window `{}` must be greater than isolation threshold `{}`",
            policy.recovery_window_ticks, policy.isolation_threshold_ticks
        ));
    }
    Ok(())
}

/// Returns whether a fault-state transition is legal for a VM node.
fn is_valid_fault_transition(
    previous: VmDistributedFaultState,
    next: VmDistributedFaultState,
) -> bool {
    use VmDistributedFaultState::{Degraded, Isolated, Recovered, Recovering, Suspected};
    matches!(
        (previous, next),
        (Recovered, Suspected)
            | (Suspected, Degraded)
            | (Suspected, Isolated)
            | (Suspected, Recovered)
            | (Degraded, Isolated)
            | (Degraded, Recovering)
            | (Degraded, Recovered)
            | (Isolated, Recovering)
            | (Recovering, Recovered)
            | (Recovering, Isolated)
    )
}
