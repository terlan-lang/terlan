//! Immutable placement and migration-plan transitions using the shared VM model.
use super::*;

impl VmSchedulingRuntime {
    pub(super) fn call_scheduler(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<ReplValue> {
        let resource = match (operation, args) {
            ("node", [id, status]) => Resource::Node(node(
                nonempty(id)?,
                match text(status)? {
                    "active" => VmClusterNodeState::Active,
                    "left" => VmClusterNodeState::Left,
                    "unreachable" => VmClusterNodeState::Unreachable,
                    "fenced" => VmClusterNodeState::Fenced,
                    _ => return Err("error[vm.scheduling.node]: unknown membership state".into()),
                },
                0,
            )),
            ("new", [nodes]) => Resource::Scheduler(Box::new(Scheduler::from_membership(
                self.nodes(owner, nodes)?,
            )?)),
            ("round_robin", []) => Resource::Policy(Policy::RoundRobin),
            ("least_connections", []) => Resource::Policy(Policy::LeastConnections),
            ("pinned", [id]) => Resource::Policy(Policy::Pinned {
                node_id: nonempty(id)?.into(),
            }),
            ("shard_affinity", [key, fallback]) => Resource::Policy(Policy::ShardAffinity {
                shard_key: nonempty(key)?.into(),
                fallback: match text(fallback)? {
                    "reject" => VmPlacementFallback::Reject,
                    "round_robin" => VmPlacementFallback::RoundRobin,
                    _ => {
                        return Err("error[vm.scheduling.fallback]: unknown fallback policy".into())
                    }
                },
            }),
            (_, [value]) => return self.scheduler_projection(owner, operation, value),
            (_, [value, rest @ ..]) => {
                let Resource::Scheduler(scheduler) = self.resource(owner, value)? else {
                    return Err(kind_error());
                };
                let mut scheduler = scheduler.clone();
                match (operation, rest) {
                    ("events_after", [cursor]) => {
                        let events = scheduler.events_after(number(cursor)?);
                        return Ok(ReplValue::List(
                            events
                                .into_iter()
                                .map(|event| self.insert(owner, Resource::Event(event)))
                                .collect::<VmRuntimeResult<Vec<_>>>()?,
                        ));
                    }
                    ("update_load", [node, count]) => scheduler.update_load(
                        nonempty(node)?,
                        usize::try_from(number(count)?)
                            .map_err(|_| "error[vm.scheduling.load]: load exceeds host range")?,
                    )?,
                    ("refresh", [nodes]) => {
                        scheduler.refresh_membership(self.nodes(owner, nodes)?)?
                    }
                    ("declare_route_policy", [route, policy]) => scheduler
                        .declare_route_policy(nonempty(route)?, self.policy(owner, policy)?)?,
                    ("declare_actor_group_policy", [route, group, policy]) => scheduler
                        .declare_actor_group_policy(
                            nonempty(route)?,
                            nonempty(group)?,
                            self.policy(owner, policy)?,
                        )?,
                    ("place", [actor, policy]) => {
                        let placement =
                            scheduler.place(nonempty(actor)?, &self.policy(owner, policy)?)?;
                        return self.scheduler_result(
                            owner,
                            scheduler,
                            Resource::Placement(placement),
                        );
                    }
                    ("place_for_route", [actor, route, policy]) => {
                        let placement = scheduler.place_for_route(
                            nonempty(actor)?,
                            nonempty(route)?,
                            &self.policy(owner, policy)?,
                        )?;
                        return self.scheduler_result(
                            owner,
                            scheduler,
                            Resource::Placement(placement),
                        );
                    }
                    ("place_for_actor_group", [actor, route, group, policy]) => {
                        let placement = scheduler.place_for_actor_group(
                            nonempty(actor)?,
                            nonempty(route)?,
                            nonempty(group)?,
                            &self.policy(owner, policy)?,
                        )?;
                        return self.scheduler_result(
                            owner,
                            scheduler,
                            Resource::Placement(placement),
                        );
                    }
                    ("request_migration", [actor, from, to, stateful]) => {
                        let migration = scheduler.request_migration(
                            nonempty(actor)?,
                            nonempty(from)?,
                            nonempty(to)?,
                            boolean(stateful)?,
                        )?;
                        return self.scheduler_result(
                            owner,
                            scheduler,
                            Resource::Migration(migration),
                        );
                    }
                    ("advance_migration", [migration, next]) => {
                        let migration = self.migration(owner, migration, &scheduler)?;
                        let migration = scheduler.advance_migration(
                            &migration.actor_id,
                            migration.sequence,
                            phase(next)?,
                        )?;
                        return self.scheduler_result(
                            owner,
                            scheduler,
                            Resource::Migration(migration),
                        );
                    }
                    ("commit_migration", [migration]) => {
                        let migration = self.migration(owner, migration, &scheduler)?;
                        let outcome =
                            scheduler.commit_migration(&migration.actor_id, migration.sequence)?;
                        return self.scheduler_result(owner, scheduler, Resource::Outcome(outcome));
                    }
                    ("rollback_migration" | "abort_migration", [migration, reason]) => {
                        let migration = self.migration(owner, migration, &scheduler)?;
                        let outcome = if operation == "rollback_migration" {
                            scheduler.rollback_migration(
                                &migration.actor_id,
                                migration.sequence,
                                nonempty(reason)?,
                            )?
                        } else {
                            scheduler.abort_migration(
                                &migration.actor_id,
                                migration.sequence,
                                nonempty(reason)?,
                            )?
                        };
                        return self.scheduler_result(owner, scheduler, Resource::Outcome(outcome));
                    }
                    _ => return Err(operation_error()),
                }
                Resource::Scheduler(scheduler)
            }
            _ => return Err(operation_error()),
        };
        self.insert(owner, resource)
    }

    fn scheduler_result(
        &mut self,
        owner: u64,
        scheduler: Box<Scheduler>,
        value: Resource,
    ) -> VmRuntimeResult<ReplValue> {
        let kind = value.identity().1;
        let scheduler = self.insert(owner, Resource::Scheduler(scheduler))?;
        let value = self.insert(owner, value)?;
        let result = match kind {
            "Placement" => Resource::PlacementResult(scheduler, value),
            "Migration" => Resource::MigrationResult(scheduler, value),
            "MigrationOutcome" => Resource::OutcomeResult(scheduler, value),
            _ => return Err(kind_error()),
        };
        self.insert(owner, result)
    }

    fn scheduler_projection(
        &self,
        owner: u64,
        operation: &str,
        value: &ReplValue,
    ) -> VmRuntimeResult<ReplValue> {
        let result = match (operation, self.resource(owner, value)?) {
            ("placement_scheduler", Resource::PlacementResult(scheduler, _))
            | ("migration_scheduler", Resource::MigrationResult(scheduler, _))
            | ("outcome_scheduler", Resource::OutcomeResult(scheduler, _)) => scheduler.clone(),
            ("placement", Resource::PlacementResult(_, value))
            | ("migration", Resource::MigrationResult(_, value))
            | ("outcome", Resource::OutcomeResult(_, value)) => value.clone(),
            ("placement_actor_id", Resource::Placement(value)) => string(&value.actor_id),
            ("placement_node_id", Resource::Placement(value)) => string(&value.node_id),
            ("placement_policy", Resource::Placement(value)) => string(value.policy),
            ("placement_fallback_used", Resource::Placement(value)) => {
                ReplValue::Bool(value.fallback_used)
            }
            ("migration_actor_id", Resource::Migration(value)) => string(&value.actor_id),
            ("migration_from_node_id", Resource::Migration(value)) => string(&value.from_node_id),
            ("migration_to_node_id", Resource::Migration(value)) => string(&value.to_node_id),
            ("migration_sequence", Resource::Migration(value)) => return integer(value.sequence),
            ("migration_stateful", Resource::Migration(value)) => ReplValue::Bool(value.stateful),
            ("migration_phase", Resource::Migration(value)) => string(phase_name(value.phase)),
            ("outcome_kind", Resource::Outcome(value)) => string(match value {
                Outcome::Committed { .. } => "committed",
                Outcome::RolledBack { .. } => "rolled_back",
                Outcome::Aborted { .. } => "aborted",
            }),
            (
                "outcome_sequence",
                Resource::Outcome(
                    Outcome::Committed { sequence }
                    | Outcome::RolledBack { sequence, .. }
                    | Outcome::Aborted { sequence, .. },
                ),
            ) => return integer(*sequence),
            ("outcome_reason", Resource::Outcome(value)) => string(match value {
                Outcome::Committed { .. } => "",
                Outcome::RolledBack { reason, .. } | Outcome::Aborted { reason, .. } => reason,
            }),
            _ => return Err(operation_error()),
        };
        Ok(result)
    }
}
