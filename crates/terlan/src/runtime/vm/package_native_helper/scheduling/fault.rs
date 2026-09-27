//! Mutable, actor-scoped fault monitoring through the shared state machine.
use super::*;

impl VmSchedulingRuntime {
    pub(super) fn call_fault(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<ReplValue> {
        let resource = match (operation, args) {
            ("policy", [suspicion, isolation, recovery]) => Resource::FaultPolicy(
                FaultPolicy::new(number(suspicion)?, number(isolation)?, number(recovery)?)?,
            ),
            ("resolve_policy", [local, peer]) => Resource::FaultPolicy(
                self.fault_policy(owner, local)?
                    .resolve(self.fault_policy(owner, peer)?),
            ),
            ("compatibility", [partition_tolerant, local_fallback]) => {
                return Ok(string(
                    match distributed_fault_compatibility(
                        boolean(partition_tolerant)?,
                        boolean(local_fallback)?,
                    ) {
                        VmDistributedCompatibilityOutcome::Supported => "supported",
                        VmDistributedCompatibilityOutcome::FallbackLocalOnly => {
                            "fallback_local_only"
                        }
                        VmDistributedCompatibilityOutcome::FeatureUnsupported => {
                            "feature_unsupported"
                        }
                    },
                ))
            }
            ("monitor", [nodes, policy]) => {
                let nodes = unique_nodes(
                    list(nodes)?
                        .iter()
                        .map(|id| Ok(node(nonempty(id)?, VmClusterNodeState::Active, 0)))
                        .collect::<VmRuntimeResult<Vec<_>>>()?,
                )?;
                Resource::Monitor(Box::new(
                    Scheduler::from_membership_with_limits_and_fault_policy(
                        nodes,
                        VmSchedulingLimits::default(),
                        self.fault_policy(owner, policy)?,
                    )?,
                ))
            }
            ("record_heartbeat", [monitor, node, tick]) => {
                let result = self
                    .monitor_mut(owner, monitor)?
                    .record_fault_heartbeat_at_tick(nonempty(node)?, number(tick)?)?;
                return Ok(string(match result {
                    VmDistributedHeartbeatObservation::Recorded { .. } => "recorded",
                    VmDistributedHeartbeatObservation::DuplicateSuppressed { .. } => {
                        "duplicate_suppressed"
                    }
                }));
            }
            ("suspect" | "isolate" | "expire", [monitor, node, tick, reason]) => {
                let monitor = self.monitor_mut(owner, monitor)?;
                let (node, tick, reason) = (nonempty(node)?, number(tick)?, nonempty(reason)?);
                let transition = match operation {
                    "suspect" => monitor.suspect_missed_heartbeat_at_tick(node, tick, reason)?,
                    "isolate" => monitor.isolate_missed_heartbeat_at_tick(node, tick, reason)?,
                    _ => monitor.expire_recovery_window_at_tick(node, tick, reason)?,
                };
                return match transition {
                    Some(transition) => Ok(ReplValue::Record {
                        name: "Some".into(),
                        fields: vec![(
                            "value".into(),
                            self.insert(owner, Resource::Transition(transition))?,
                        )],
                    }),
                    None => Ok(ReplValue::Record {
                        name: "None".into(),
                        fields: Vec::new(),
                    }),
                };
            }
            ("degrade" | "begin_recovery" | "complete", [monitor, node, tick, reason]) => {
                let monitor = self.monitor_mut(owner, monitor)?;
                let (node, tick, reason) = (nonempty(node)?, number(tick)?, nonempty(reason)?);
                Resource::Transition(if operation == "complete" {
                    monitor.complete_recovery_at_tick(node, tick, reason)?
                } else {
                    monitor.transition_fault_state_at_tick(
                        node,
                        if operation == "degrade" {
                            State::Degraded
                        } else {
                            State::Recovering
                        },
                        tick,
                        reason,
                    )?
                })
            }
            ("state_name", [monitor, node]) => {
                let Resource::Monitor(monitor) = self.resource(owner, monitor)? else {
                    return Err(kind_error());
                };
                return Ok(string(state_name(
                    monitor
                        .fault_state(nonempty(node)?)
                        .ok_or("error[vm.scheduling.node]: node is not monitored")?,
                )));
            }
            ("transitions_after" | "failures_after", [monitor, cursor]) => {
                let Resource::Monitor(monitor) = self.resource(owner, monitor)? else {
                    return Err(kind_error());
                };
                let cursor = number(cursor)?;
                let resources = if operation == "transitions_after" {
                    monitor
                        .fault_transitions_after(cursor)
                        .into_iter()
                        .map(Resource::Transition)
                        .collect::<Vec<_>>()
                } else {
                    monitor
                        .failure_envelopes_after(cursor)
                        .into_iter()
                        .map(Resource::Failure)
                        .collect()
                };
                return Ok(ReplValue::List(
                    resources
                        .into_iter()
                        .map(|value| self.insert(owner, value))
                        .collect::<VmRuntimeResult<Vec<_>>>()?,
                ));
            }
            ("close", [monitor]) => {
                if !matches!(self.resource(owner, monitor)?, Resource::Monitor(_)) {
                    return Err(kind_error());
                }
                self.resources
                    .dispose_for_owner(self.handle(owner, monitor)?, owner)
                    .map_err(resource_error)?;
                return Ok(ReplValue::Unit);
            }
            _ => return self.fault_descriptor(owner, operation, args),
        };
        self.insert(owner, resource)
    }
}
