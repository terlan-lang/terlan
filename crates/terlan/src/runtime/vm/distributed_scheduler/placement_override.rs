use std::collections::BTreeMap;

use super::{VmDistributedScheduler, VmPlacementDecision, VmPlacementPolicy};

impl VmDistributedScheduler {
    /// Declares an immutable route-level placement policy override.
    pub(crate) fn declare_route_policy(
        &mut self,
        route_id: impl Into<String>,
        policy: VmPlacementPolicy,
    ) -> Result<(), String> {
        let route_id = route_id.into();
        validate_scope_id(&route_id, "route")?;
        validate_policy(&policy)?;
        insert_override(&mut self.route_policy_overrides, route_id, policy, "route")
    }

    /// Declares an immutable actor-group policy scoped to one route.
    pub(crate) fn declare_actor_group_policy(
        &mut self,
        route_id: impl Into<String>,
        actor_group_id: impl Into<String>,
        policy: VmPlacementPolicy,
    ) -> Result<(), String> {
        let route_id = route_id.into();
        let actor_group_id = actor_group_id.into();
        validate_scope_id(&route_id, "route")?;
        validate_scope_id(&actor_group_id, "actor group")?;
        validate_policy(&policy)?;
        let route_groups = self
            .actor_group_policy_overrides
            .entry(route_id)
            .or_default();
        insert_override(route_groups, actor_group_id, policy, "actor-group")
    }

    /// Places an actor with a route override taking precedence over the default.
    pub(crate) fn place_for_route(
        &mut self,
        actor_id: impl Into<String>,
        route_id: &str,
        default_policy: &VmPlacementPolicy,
    ) -> Result<VmPlacementDecision, String> {
        validate_scope_id(route_id, "route")?;
        validate_policy(default_policy)?;
        let policy = self
            .route_policy_overrides
            .get(route_id)
            .cloned()
            .unwrap_or_else(|| default_policy.clone());
        self.place(actor_id, &policy)
    }

    /// Places an actor with actor-group, route, then default policy precedence.
    pub(crate) fn place_for_actor_group(
        &mut self,
        actor_id: impl Into<String>,
        route_id: &str,
        actor_group_id: &str,
        default_policy: &VmPlacementPolicy,
    ) -> Result<VmPlacementDecision, String> {
        validate_scope_id(route_id, "route")?;
        validate_scope_id(actor_group_id, "actor group")?;
        validate_policy(default_policy)?;
        let policy = self
            .actor_group_policy_overrides
            .get(route_id)
            .and_then(|groups| groups.get(actor_group_id))
            .or_else(|| self.route_policy_overrides.get(route_id))
            .cloned()
            .unwrap_or_else(|| default_policy.clone());
        self.place(actor_id, &policy)
    }
}

pub(super) fn validate_policy(policy: &VmPlacementPolicy) -> Result<(), String> {
    match policy {
        VmPlacementPolicy::Pinned { node_id } if node_id.is_empty() => Err(
            "error[vm_distributed_scheduler]: pinned policy node id must be non-empty".to_string(),
        ),
        VmPlacementPolicy::ShardAffinity { shard_key, .. } if shard_key.is_empty() => {
            Err("error[vm_distributed_scheduler]: shard policy key must be non-empty".to_string())
        }
        _ => Ok(()),
    }
}

fn validate_scope_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!(
            "error[vm_distributed_scheduler]: {label} id must be non-empty"
        ));
    }
    Ok(())
}

fn insert_override<K>(
    overrides: &mut BTreeMap<K, VmPlacementPolicy>,
    key: K,
    policy: VmPlacementPolicy,
    label: &str,
) -> Result<(), String>
where
    K: Ord,
{
    match overrides.get(&key) {
        Some(existing) if existing == &policy => Ok(()),
        Some(_) => Err(format!(
            "error[vm_distributed_scheduler]: conflicting {label} policy override"
        )),
        None => {
            overrides.insert(key, policy);
            Ok(())
        }
    }
}
