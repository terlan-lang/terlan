//! Validated immutable diagnostic descriptors; these never perform migration.
use super::*;

impl VmSchedulingRuntime {
    pub(super) fn fault_descriptor(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<ReplValue> {
        let resource = match (operation, args) {
            ("state", [node, status]) => Resource::State(nonempty(node)?.into(), state(status)?),
            ("classify_heartbeat" | "isolate_partition", [policy, node, tick, gap, reason]) => {
                let policy = self.fault_policy(owner, policy)?;
                let (tick, gap) = (number(tick)?, number(gap)?);
                let threshold = if operation == "classify_heartbeat" {
                    policy.suspicion_threshold_ticks
                } else {
                    policy.isolation_threshold_ticks
                };
                if gap > tick || gap <= threshold {
                    return Err("error[vm.scheduling.threshold]: heartbeat gap does not establish the requested transition".into());
                }
                Resource::Transition(Transition {
                    node_id: nonempty(node)?.into(),
                    previous_state: if operation == "classify_heartbeat" {
                        State::Recovered
                    } else {
                        State::Suspected
                    },
                    next_state: if operation == "classify_heartbeat" {
                        State::Suspected
                    } else {
                        State::Isolated
                    },
                    tick,
                    reason: nonempty(reason)?.into(),
                })
            }
            ("start_recovery", [node, tick, reason]) => {
                nonempty(reason)?;
                Resource::Recovery(nonempty(node)?.into(), number(tick)?)
            }
            ("complete_recovery", [recovery, tick, reason]) => {
                let Resource::Recovery(node, start) = self.resource(owner, recovery)? else {
                    return Err(kind_error());
                };
                let tick = number(tick)?;
                if tick < *start {
                    return Err("error[vm.scheduling.tick]: completion predates recovery".into());
                }
                Resource::Transition(Transition {
                    node_id: node.clone(),
                    previous_state: State::Recovering,
                    next_state: State::Recovered,
                    tick,
                    reason: nonempty(reason)?.into(),
                })
            }
            ("expire_recovery", [policy, recovery, tick, reason]) => {
                let policy = self.fault_policy(owner, policy)?;
                let Resource::Recovery(node, start) = self.resource(owner, recovery)? else {
                    return Err(kind_error());
                };
                let tick = number(tick)?;
                if tick
                    .checked_sub(*start)
                    .is_none_or(|gap| gap <= policy.recovery_window_ticks)
                {
                    return Err("error[vm.scheduling.tick]: recovery window has not expired".into());
                }
                Resource::Failure(Failure {
                    node_id: node.clone(),
                    tick,
                    kind: FailureKind::RecoveryWindowExpired {
                        node_id: node.clone(),
                        recovery_started_tick: *start,
                        current_tick: tick,
                        window_ticks: policy.recovery_window_ticks,
                    },
                    reason: nonempty(reason)?.into(),
                })
            }
            (
                "migration_timeout" | "migration_partial_commit",
                [actor, sequence, from, to, stage, tick, reason],
            ) => {
                let actor_id = nonempty(actor)?.to_owned();
                let migration_sequence = number(sequence)?;
                if migration_sequence == 0 || nonempty(from)? == nonempty(to)? {
                    return Err("error[vm.scheduling.migration]: expected a nonzero sequence and distinct nodes".into());
                }
                let phase = phase(stage)?;
                let kind = if operation == "migration_timeout" {
                    FailureKind::MigrationTimeout {
                        actor_id,
                        migration_sequence,
                        phase,
                    }
                } else {
                    FailureKind::MigrationPartialCommit {
                        actor_id,
                        migration_sequence,
                        phase,
                    }
                };
                Resource::Rollback(Failure {
                    node_id: text(to)?.into(),
                    tick: number(tick)?,
                    kind,
                    reason: nonempty(reason)?.into(),
                })
            }
            ("failure", [rollback]) => {
                let Resource::Rollback(failure) = self.resource(owner, rollback)? else {
                    return Err(kind_error());
                };
                Resource::Failure(failure.clone())
            }
            ("stale_placement_update", [actor, node, incoming, current, tick, reason]) => {
                let (incoming_sequence, current_sequence) = (number(incoming)?, number(current)?);
                if incoming_sequence >= current_sequence {
                    return Err(
                        "error[vm.scheduling.sequence]: placement update is not stale".into(),
                    );
                }
                Resource::Failure(Failure {
                    node_id: nonempty(node)?.into(),
                    tick: number(tick)?,
                    kind: FailureKind::StalePlacementUpdate {
                        actor_id: nonempty(actor)?.into(),
                        incoming_sequence,
                        current_sequence,
                    },
                    reason: nonempty(reason)?.into(),
                })
            }
            (_, [value]) => return self.fault_projection(owner, operation, value),
            _ => return Err(operation_error()),
        };
        self.insert(owner, resource)
    }

    fn fault_projection(
        &self,
        owner: u64,
        operation: &str,
        value: &ReplValue,
    ) -> VmRuntimeResult<ReplValue> {
        let value = match (operation, self.resource(owner, value)?) {
            ("transition_node_id", Resource::Transition(value)) => string(&value.node_id),
            ("transition_previous_state", Resource::Transition(value)) => {
                string(state_name(value.previous_state))
            }
            ("transition_next_state", Resource::Transition(value)) => {
                string(state_name(value.next_state))
            }
            ("transition_tick", Resource::Transition(value)) => return integer(value.tick),
            ("transition_reason", Resource::Transition(value)) => string(&value.reason),
            ("transition_diagnostic_kind", Resource::Transition(value)) => {
                string(value.diagnostic_kind())
            }
            ("failure_kind", Resource::Failure(value)) => string(value.kind.label()),
            ("failure_node_id", Resource::Failure(value)) => string(&value.node_id),
            ("failure_tick", Resource::Failure(value)) => return integer(value.tick),
            ("failure_reason", Resource::Failure(value)) => string(&value.reason),
            ("failure_diagnostic_kind", Resource::Failure(value)) => string(match value.kind {
                FailureKind::HeartbeatMissed { .. } => "partition_onset",
                FailureKind::PartitionSuspected { .. } => "node_role_demotion",
                FailureKind::RecoveryWindowExpired { .. } => "recovery_window_expired",
                FailureKind::MigrationTimeout { .. }
                | FailureKind::MigrationPartialCommit { .. } => "migration_rollback_decision",
                FailureKind::StalePlacementUpdate { .. } => "stale_placement_update",
            }),
            _ => return Err(operation_error()),
        };
        Ok(value)
    }
}
