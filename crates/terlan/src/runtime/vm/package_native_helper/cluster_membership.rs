//! Immutable source views over the existing VM membership state machine.

use crate::runtime::vm::coordination_membership::{VmClusterNodeSnapshot, VmClusterNodeState};

use super::{
    text, ClusterResource, ReplValue, VmClusterMembership, VmClusterRuntime, VmRuntimeResult,
    MEMBERSHIP,
};

impl VmClusterRuntime {
    /// Dispatches the closed membership operation family without external helpers.
    pub(super) fn membership_call(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<ReplValue> {
        let updated = match (operation, args) {
            ("std.vm.cluster.membership", [profile, timeout]) => {
                VmClusterMembership::new(self.profile(owner, profile)?.clone(), tick(timeout)?)?
            }
            ("std.vm.cluster.membership_join", [membership, peer, now, role]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.join_peer(self.profile(owner, peer)?, tick(now)?, [text(role)?])?;
                updated
            }
            ("std.vm.cluster.membership_restart", [membership, peer, now]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.restart_peer(self.profile(owner, peer)?, tick(now)?)?;
                updated
            }
            ("std.vm.cluster.membership_heartbeat", [membership, node, now]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.record_heartbeat(text(node)?, tick(now)?)?;
                updated
            }
            ("std.vm.cluster.membership_partition", [membership, node, now]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.partition_node(text(node)?, tick(now)?)?;
                updated
            }
            ("std.vm.cluster.membership_heal", [membership, node, now]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.heal_node(text(node)?, tick(now)?)?;
                updated
            }
            ("std.vm.cluster.membership_leave", [membership, node, now]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.mark_left(text(node)?, tick(now)?)?;
                updated
            }
            ("std.vm.cluster.membership_fence", [membership, node, now]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.fence_node(text(node)?, tick(now)?)?;
                updated
            }
            ("std.vm.cluster.membership_expire", [membership, now]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.expire_stale_nodes(tick(now)?);
                updated
            }
            ("std.vm.cluster.membership_prune", [membership, now, retention]) => {
                let mut updated = self.membership(owner, membership)?.clone();
                updated.prune_stale_nodes(tick(now)?, tick(retention)?)?;
                updated
            }
            ("std.vm.cluster.membership_state", [membership, node]) => {
                return Ok(state(
                    self.membership(owner, membership)?
                        .node(text(node)?)
                        .map(|n| n.state),
                ));
            }
            ("std.vm.cluster.membership_view", [membership]) => {
                return self
                    .membership(owner, membership)?
                    .view()
                    .into_iter()
                    .map(snapshot)
                    .collect::<VmRuntimeResult<Vec<_>>>()
                    .map(ReplValue::List);
            }
            ("std.vm.cluster.membership_health", [membership]) => {
                let healthy = self
                    .membership(owner, membership)?
                    .view()
                    .iter()
                    .all(|node| node.state == VmClusterNodeState::Active);
                return Ok(ReplValue::Atom(
                    if healthy { "healthy" } else { "degraded" }.into(),
                ));
            }
            ("std.vm.cluster.membership_leader_hint", [membership]) => {
                let leader = self
                    .membership(owner, membership)?
                    .view()
                    .into_iter()
                    .find(|node| {
                        node.state == VmClusterNodeState::Active
                            && node.role_tags.iter().any(|role| role == "leader")
                    });
                return Ok(match leader {
                    Some(node) => ReplValue::Record {
                        name: "Some".into(),
                        fields: vec![("value".into(), ReplValue::String(node.node_id))],
                    },
                    None => singleton("None"),
                });
            }
            _ => {
                return Err(format!(
                "error[vm.cluster.operation]: unsupported membership operation or arity `{}/{}`",
                operation, args.len(),
            )
                .into())
            }
        };
        self.insert_resource(owner, ClusterResource::Membership(updated))
    }

    fn membership(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<&VmClusterMembership> {
        match self.resource(owner, value, MEMBERSHIP)? {
            ClusterResource::Membership(membership) => Ok(membership),
            _ => Err("error[vm.cluster.kind]: stored resource is not a Membership".into()),
        }
    }
}

fn tick(value: &ReplValue) -> VmRuntimeResult<u64> {
    match value {
        ReplValue::Int(value) => u64::try_from(*value)
            .map_err(|_| "error[vm.cluster.tick]: expected a non-negative tick".into()),
        _ => Err("error[vm.cluster.tick]: expected an Int tick".into()),
    }
}

fn singleton(name: &str) -> ReplValue {
    ReplValue::Record {
        name: name.into(),
        fields: Vec::new(),
    }
}

fn state(value: Option<VmClusterNodeState>) -> ReplValue {
    ReplValue::Atom(
        match value {
            Some(VmClusterNodeState::Active) => "active",
            Some(VmClusterNodeState::Left) => "left",
            Some(VmClusterNodeState::Unreachable) => "unreachable",
            Some(VmClusterNodeState::Fenced) => "fenced",
            None => "missing",
        }
        .into(),
    )
}

fn snapshot(node: VmClusterNodeSnapshot) -> VmRuntimeResult<ReplValue> {
    let last_seen = i64::try_from(node.last_seen_tick)
        .map_err(|_| "error[vm.cluster.tick]: snapshot tick exceeds Terlan Int")?;
    Ok(ReplValue::Record {
        name: "Node".into(),
        fields: vec![
            ("node_id".into(), ReplValue::String(node.node_id)),
            ("state".into(), state(Some(node.state))),
            ("last_seen_tick".into(), ReplValue::Int(last_seen)),
            (
                "roles".into(),
                ReplValue::List(node.role_tags.into_iter().map(ReplValue::String).collect()),
            ),
        ],
    })
}
