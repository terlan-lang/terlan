//! Checked scalar and descriptor conversion for the local scheduling APIs.
use super::*;

pub(super) fn text(value: &ReplValue) -> VmRuntimeResult<&str> {
    match value {
        ReplValue::String(value) => Ok(value),
        ReplValue::StringBytes(value) => std::str::from_utf8(value)
            .map_err(|_| "error[vm.scheduling.text]: expected UTF-8".into()),
        _ => Err("error[vm.scheduling.text]: expected String".into()),
    }
}

pub(super) fn nonempty(value: &ReplValue) -> VmRuntimeResult<&str> {
    let value = text(value)?;
    if value.is_empty() {
        Err("error[vm.scheduling.text]: expected nonempty text".into())
    } else {
        Ok(value)
    }
}

pub(super) fn number(value: &ReplValue) -> VmRuntimeResult<u64> {
    match value {
        ReplValue::Int(value) => u64::try_from(*value)
            .map_err(|_| "error[vm.scheduling.number]: expected nonnegative Int".into()),
        _ => Err("error[vm.scheduling.number]: expected Int".into()),
    }
}

pub(super) fn boolean(value: &ReplValue) -> VmRuntimeResult<bool> {
    match value {
        ReplValue::Bool(value) => Ok(*value),
        _ => Err("error[vm.scheduling.boolean]: expected Bool".into()),
    }
}

pub(super) fn list(value: &ReplValue) -> VmRuntimeResult<&[ReplValue]> {
    match value {
        ReplValue::List(value) => Ok(value),
        _ => Err("error[vm.scheduling.list]: expected List".into()),
    }
}

pub(super) fn integer(value: u64) -> VmRuntimeResult<ReplValue> {
    Ok(ReplValue::Int(i64::try_from(value).map_err(|_| {
        "error[vm.scheduling.number]: exceeds Int range"
    })?))
}

pub(super) fn string(value: impl Into<String>) -> ReplValue {
    ReplValue::String(value.into())
}

pub(super) fn phase(value: &ReplValue) -> VmRuntimeResult<Phase> {
    match text(value)? {
        "requested" => Ok(Phase::Requested),
        "snapshotting" => Ok(Phase::Snapshotting),
        "transferring" => Ok(Phase::Transferring),
        "resuming" => Ok(Phase::Resuming),
        _ => Err("error[vm.scheduling.phase]: unknown migration phase".into()),
    }
}

pub(super) fn phase_name(value: Phase) -> &'static str {
    match value {
        Phase::Requested => "requested",
        Phase::Snapshotting => "snapshotting",
        Phase::Transferring => "transferring",
        Phase::Resuming => "resuming",
    }
}

pub(super) fn state(value: &ReplValue) -> VmRuntimeResult<State> {
    match text(value)? {
        "recovered" => Ok(State::Recovered),
        "suspected" => Ok(State::Suspected),
        "degraded" => Ok(State::Degraded),
        "isolated" => Ok(State::Isolated),
        "recovering" => Ok(State::Recovering),
        _ => Err("error[vm.scheduling.state]: unknown fault state".into()),
    }
}

pub(super) fn state_name(value: State) -> &'static str {
    match value {
        State::Recovered => "recovered",
        State::Suspected => "suspected",
        State::Degraded => "degraded",
        State::Isolated => "isolated",
        State::Recovering => "recovering",
    }
}

pub(super) fn node(id: &str, state: VmClusterNodeState, tick: u64) -> Node {
    Node {
        app_id: String::new(),
        vm_id: String::new(),
        node_id: id.into(),
        state,
        last_seen_tick: tick,
        role_tags: Vec::new(),
    }
}

pub(super) fn unique_nodes(nodes: Vec<Node>) -> VmRuntimeResult<Vec<Node>> {
    let mut identities = std::collections::BTreeSet::new();
    if nodes.is_empty()
        || nodes
            .iter()
            .any(|node| node.node_id.is_empty() || !identities.insert(&node.node_id))
    {
        return Err("error[vm.scheduling.nodes]: expected nonempty unique node identities".into());
    }
    Ok(nodes)
}

impl VmSchedulingRuntime {
    pub(super) fn policy(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<Policy> {
        match self.resource(owner, value)? {
            Resource::Policy(value) => Ok(value.clone()),
            _ => Err(kind_error()),
        }
    }

    pub(super) fn fault_policy(
        &self,
        owner: u64,
        value: &ReplValue,
    ) -> VmRuntimeResult<FaultPolicy> {
        match self.resource(owner, value)? {
            Resource::FaultPolicy(value) => Ok(*value),
            _ => Err(kind_error()),
        }
    }

    pub(super) fn nodes(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<Vec<Node>> {
        unique_nodes(
            list(value)?
                .iter()
                .map(|value| match self.resource(owner, value)? {
                    Resource::Node(node) => Ok(node.clone()),
                    _ => Err(kind_error()),
                })
                .collect::<VmRuntimeResult<Vec<_>>>()?,
        )
    }

    pub(super) fn migration(
        &self,
        owner: u64,
        value: &ReplValue,
        scheduler: &Scheduler,
    ) -> VmRuntimeResult<Migration> {
        match self.resource(owner, value)? {
            Resource::Migration(value) => {
                if scheduler
                    .migration_intent(&value.actor_id)
                    .is_some_and(|actual| actual != value)
                {
                    return Err(
                        "error[vm.scheduling.migration]: descriptor does not match the plan".into(),
                    );
                }
                Ok(value.clone())
            }
            _ => Err(kind_error()),
        }
    }
}
