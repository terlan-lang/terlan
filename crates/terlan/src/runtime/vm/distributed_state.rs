use super::ReplValue;
#[cfg(test)]
use std::collections::BTreeMap;

/// Conflict strategy attached to a VM-owned distributed state entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedStatePolicy {
    WinnerTakesAll,
    LastWriterWins,
    Merge,
    ExplicitUserResolution,
}

/// Namespaced distributed state key owned by the VM runtime.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmDistributedStateScope {
    pub(crate) namespace: String,
    pub(crate) key: String,
}

#[cfg(test)]
impl VmDistributedStateScope {
    /// Builds a validated state scope from namespace and key text.
    pub(crate) fn new(
        namespace: impl Into<String>,
        key: impl Into<String>,
    ) -> Result<Self, String> {
        let namespace = namespace.into();
        let key = key.into();
        if namespace.is_empty() {
            return Err(
                "error[vm_distributed_state]: state namespace must be non-empty".to_string(),
            );
        }
        if key.is_empty() {
            return Err("error[vm_distributed_state]: state key must be non-empty".to_string());
        }
        Ok(Self { namespace, key })
    }
}

/// Monotonic version tag for one distributed state write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedStateVersion {
    pub(crate) sequence: u64,
    pub(crate) node_id: String,
}

#[cfg(test)]
impl VmDistributedStateVersion {
    /// Builds a validated version from a sequence and writer node id.
    pub(crate) fn new(sequence: u64, node_id: impl Into<String>) -> Result<Self, String> {
        let node_id = node_id.into();
        if sequence == 0 {
            return Err(
                "error[vm_distributed_state]: state version sequence must be non-zero".to_string(),
            );
        }
        if node_id.is_empty() {
            return Err(
                "error[vm_distributed_state]: state version node id must be non-empty".to_string(),
            );
        }
        Ok(Self { sequence, node_id })
    }
}

/// VM-owned distributed state entry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmDistributedStateEntry {
    pub(crate) scope: VmDistributedStateScope,
    pub(crate) owner_node_id: String,
    pub(crate) value: ReplValue,
    pub(crate) version: VmDistributedStateVersion,
    pub(crate) policy: VmDistributedStatePolicy,
}

/// Conflict metadata returned when a write cannot deterministically apply.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDistributedStateConflict {
    pub(crate) scope: VmDistributedStateScope,
    pub(crate) local_version: VmDistributedStateVersion,
    pub(crate) incoming_version: VmDistributedStateVersion,
    pub(crate) policy: VmDistributedStatePolicy,
}

/// Outcome of applying one VM-owned distributed state write.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) enum VmDistributedStateWriteOutcome {
    Applied(VmDistributedStateEntry),
    Replayed(VmDistributedStateEntry),
    Conflict(VmDistributedStateConflict),
    PolicyMismatch {
        scope: VmDistributedStateScope,
        existing_policy: VmDistributedStatePolicy,
        incoming_policy: VmDistributedStatePolicy,
    },
}

/// In-memory VM distributed state table used by replication contracts.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDistributedStateStore {
    entries: BTreeMap<VmDistributedStateScope, VmDistributedStateEntry>,
}

#[cfg(test)]
impl VmDistributedStateStore {
    /// Creates an empty VM distributed state store.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns the entry stored for a scope, if one exists.
    pub(crate) fn get(&self, scope: &VmDistributedStateScope) -> Option<&VmDistributedStateEntry> {
        self.entries.get(scope)
    }

    /// Writes a value with explicit ownership, version, and conflict policy.
    pub(crate) fn write(
        &mut self,
        scope: VmDistributedStateScope,
        owner_node_id: impl Into<String>,
        value: ReplValue,
        version: VmDistributedStateVersion,
        policy: VmDistributedStatePolicy,
    ) -> Result<VmDistributedStateWriteOutcome, String> {
        let owner_node_id = owner_node_id.into();
        if owner_node_id.is_empty() {
            return Err(
                "error[vm_distributed_state]: state owner node id must be non-empty".to_string(),
            );
        }
        let incoming = VmDistributedStateEntry {
            scope: scope.clone(),
            owner_node_id,
            value,
            version,
            policy,
        };
        match self.entries.get(&scope) {
            None => {
                self.entries.insert(scope, incoming.clone());
                Ok(VmDistributedStateWriteOutcome::Applied(incoming))
            }
            Some(existing) if existing.policy != incoming.policy => {
                Ok(VmDistributedStateWriteOutcome::PolicyMismatch {
                    scope,
                    existing_policy: existing.policy,
                    incoming_policy: incoming.policy,
                })
            }
            Some(existing) if existing.version == incoming.version && existing == &incoming => {
                Ok(VmDistributedStateWriteOutcome::Replayed(existing.clone()))
            }
            Some(existing)
                if should_apply_incoming_version(
                    &existing.version,
                    &incoming.version,
                    incoming.policy,
                ) =>
            {
                self.entries.insert(scope, incoming.clone());
                Ok(VmDistributedStateWriteOutcome::Applied(incoming))
            }
            Some(existing) => Ok(VmDistributedStateWriteOutcome::Conflict(
                VmDistributedStateConflict {
                    scope,
                    local_version: existing.version.clone(),
                    incoming_version: incoming.version,
                    policy,
                },
            )),
        }
    }

    /// Exports entries in deterministic scope order for checkpointing.
    pub(crate) fn export_snapshot(&self) -> Vec<VmDistributedStateEntry> {
        self.entries.values().cloned().collect()
    }

    /// Restores entries from a deterministic checkpoint snapshot.
    pub(crate) fn import_snapshot(
        entries: impl IntoIterator<Item = VmDistributedStateEntry>,
    ) -> Result<Self, String> {
        let mut store = Self::new();
        for entry in entries {
            if entry.scope.namespace.is_empty() || entry.scope.key.is_empty() {
                return Err("error[vm_distributed_state]: snapshot scope must be valid".to_string());
            }
            if entry.owner_node_id.is_empty() {
                return Err(
                    "error[vm_distributed_state]: snapshot owner node id must be non-empty"
                        .to_string(),
                );
            }
            if entry.version.sequence == 0 || entry.version.node_id.is_empty() {
                return Err(
                    "error[vm_distributed_state]: snapshot version must be valid".to_string(),
                );
            }
            if store.entries.insert(entry.scope.clone(), entry).is_some() {
                return Err(
                    "error[vm_distributed_state]: snapshot contains duplicate state scope"
                        .to_string(),
                );
            }
        }
        Ok(store)
    }
}

/// Returns whether an incoming write wins under the selected conflict policy.
#[cfg(test)]
fn should_apply_incoming_version(
    local: &VmDistributedStateVersion,
    incoming: &VmDistributedStateVersion,
    policy: VmDistributedStatePolicy,
) -> bool {
    policy == VmDistributedStatePolicy::LastWriterWins
        && (incoming.sequence > local.sequence
            || incoming.sequence == local.sequence && incoming.node_id > local.node_id)
}

#[cfg(test)]
#[path = "distributed_state_test.rs"]
#[cfg(test)]
mod distributed_state_test;
