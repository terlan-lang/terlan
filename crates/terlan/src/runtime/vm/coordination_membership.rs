//! Deterministic VM-owned membership state, shared with the source adapter.

use std::collections::{BTreeMap, BTreeSet};

use super::coordination_profile::VmCoordinationProfile;

/// Lifecycle state for one node in a VM cluster membership view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmClusterNodeState {
    Active,
    Left,
    Unreachable,
    Fenced,
}

/// Inspectable membership record for one VM cluster node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmClusterNodeSnapshot {
    pub(crate) app_id: String,
    pub(crate) vm_id: String,
    pub(crate) node_id: String,
    pub(crate) state: VmClusterNodeState,
    pub(crate) last_seen_tick: u64,
    pub(crate) role_tags: Vec<String>,
}

impl VmClusterNodeSnapshot {
    /// Builds a deterministic node snapshot from a coordination profile.
    fn from_profile(
        profile: &VmCoordinationProfile,
        state: VmClusterNodeState,
        last_seen_tick: u64,
        role_tags: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            app_id: profile.app_id().to_string(),
            vm_id: profile.vm_id().to_string(),
            node_id: profile.node_id().to_string(),
            state,
            last_seen_tick,
            role_tags: role_tags
                .into_iter()
                .map(Into::into)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }
}

/// VM-owned cluster membership view for transport lifecycle decisions.
///
/// Inputs:
/// - Local VM coordination profile.
/// - Heartbeat timeout in VM scheduler ticks.
///
/// Output:
/// - Deterministic membership table with node state transitions.
///
/// Transformation:
/// - Tracks join, heartbeat, leave, unreachable, and fenced states without
///   embedding a consensus algorithm or backend network transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmClusterMembership {
    local: VmCoordinationProfile,
    heartbeat_timeout_ticks: u64,
    nodes: BTreeMap<String, VmClusterNodeSnapshot>,
    node_epochs: BTreeMap<String, u64>,
}

impl VmClusterMembership {
    /// Creates a membership view containing the local node as active.
    pub(crate) fn new(
        local: VmCoordinationProfile,
        heartbeat_timeout_ticks: u64,
    ) -> Result<Self, String> {
        if heartbeat_timeout_ticks == 0 {
            return Err(
                "error[vm_cluster_membership]: heartbeat timeout ticks must be non-zero"
                    .to_string(),
            );
        }
        let mut nodes = BTreeMap::new();
        nodes.insert(
            local.node_id().to_string(),
            VmClusterNodeSnapshot::from_profile(&local, VmClusterNodeState::Active, 0, ["local"]),
        );
        let node_epochs = BTreeMap::from([(local.node_id().to_string(), local.epoch())]);
        Ok(Self {
            local,
            heartbeat_timeout_ticks,
            nodes,
            node_epochs,
        })
    }

    /// Joins a compatible peer node into this membership view.
    pub(crate) fn join_peer(
        &mut self,
        peer: &VmCoordinationProfile,
        tick: u64,
        role_tags: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<(), String> {
        if !self.local.can_coordinate_with(peer) {
            return Err(
                "error[vm_cluster_membership]: incompatible VM coordination profile".to_string(),
            );
        }
        if matches!(
            self.nodes.get(peer.node_id()).map(|node| node.state),
            Some(VmClusterNodeState::Fenced)
        ) {
            return Err(format!(
                "error[vm_cluster_membership]: fenced node `{}` cannot rejoin",
                peer.node_id()
            ));
        }
        if self.nodes.contains_key(peer.node_id()) {
            return Err(format!(
                "error[vm_cluster_membership]: node `{}` is already known; use restart with a newer epoch",
                peer.node_id()
            ));
        }
        self.nodes.insert(
            peer.node_id().to_string(),
            VmClusterNodeSnapshot::from_profile(peer, VmClusterNodeState::Active, tick, role_tags),
        );
        self.node_epochs
            .insert(peer.node_id().to_string(), peer.epoch());
        Ok(())
    }

    /// Replaces one known peer incarnation with a strictly newer epoch.
    pub(crate) fn restart_peer(
        &mut self,
        peer: &VmCoordinationProfile,
        tick: u64,
    ) -> Result<(), String> {
        if !self.local.can_coordinate_with(peer) {
            return Err(
                "error[vm_cluster_membership]: incompatible restart VM coordination profile"
                    .to_string(),
            );
        }
        let node = self.nodes.get_mut(peer.node_id()).ok_or_else(|| {
            format!(
                "error[vm_cluster_membership]: cannot restart unknown node `{}`",
                peer.node_id()
            )
        })?;
        if node.state == VmClusterNodeState::Fenced {
            return Err(format!(
                "error[vm_cluster_membership]: fenced node `{}` cannot restart",
                peer.node_id()
            ));
        }
        if node.app_id != peer.app_id() || node.vm_id != peer.vm_id() {
            return Err(format!(
                "error[vm_cluster_membership]: restart identity mismatch for node `{}`",
                peer.node_id()
            ));
        }
        let current_epoch = self
            .node_epochs
            .get(peer.node_id())
            .copied()
            .ok_or_else(|| {
                format!(
                    "error[vm_cluster_membership]: node `{}` is missing epoch state",
                    peer.node_id()
                )
            })?;
        if peer.epoch() <= current_epoch {
            return Err(format!(
                "error[vm_cluster_membership]: stale restart epoch `{}` for node `{}`; current epoch is `{current_epoch}`",
                peer.epoch(),
                peer.node_id()
            ));
        }
        node.last_seen_tick = tick.max(node.last_seen_tick);
        node.state = VmClusterNodeState::Active;
        self.node_epochs
            .insert(peer.node_id().to_string(), peer.epoch());
        Ok(())
    }

    /// Records a heartbeat and returns the resulting node state.
    pub(crate) fn record_heartbeat(
        &mut self,
        node_id: &str,
        tick: u64,
    ) -> Result<VmClusterNodeState, String> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| format!("error[vm_cluster_membership]: unknown node `{node_id}`"))?;
        if tick < node.last_seen_tick {
            return Err(format!(
                "error[vm_cluster_membership]: stale heartbeat for node `{node_id}`"
            ));
        }
        if matches!(
            node.state,
            VmClusterNodeState::Left | VmClusterNodeState::Fenced
        ) {
            return Err(format!(
                "error[vm_cluster_membership]: node `{node_id}` is not heartbeat-eligible"
            ));
        }
        node.last_seen_tick = tick;
        node.state = VmClusterNodeState::Active;
        Ok(node.state)
    }

    /// Simulates an explicit peer partition without changing stable identity.
    pub(crate) fn partition_node(&mut self, node_id: &str, tick: u64) -> Result<(), String> {
        if node_id == self.local.node_id() {
            return Err(
                "error[vm_cluster_membership]: local node cannot be partitioned through a peer view"
                    .to_string(),
            );
        }
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| format!("error[vm_cluster_membership]: unknown node `{node_id}`"))?;
        if tick < node.last_seen_tick {
            return Err(format!(
                "error[vm_cluster_membership]: stale partition tick for node `{node_id}`"
            ));
        }
        match node.state {
            VmClusterNodeState::Active => {
                node.last_seen_tick = tick;
                node.state = VmClusterNodeState::Unreachable;
                Ok(())
            }
            VmClusterNodeState::Unreachable => Err(format!(
                "error[vm_cluster_membership]: node `{node_id}` is already partitioned"
            )),
            VmClusterNodeState::Left | VmClusterNodeState::Fenced => Err(format!(
                "error[vm_cluster_membership]: node `{node_id}` is not partition-eligible"
            )),
        }
    }

    /// Heals one explicitly or timeout-unreachable peer at a monotonic tick.
    pub(crate) fn heal_node(&mut self, node_id: &str, tick: u64) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| format!("error[vm_cluster_membership]: unknown node `{node_id}`"))?;
        if tick < node.last_seen_tick {
            return Err(format!(
                "error[vm_cluster_membership]: stale heal tick for node `{node_id}`"
            ));
        }
        if node.state != VmClusterNodeState::Unreachable {
            return Err(format!(
                "error[vm_cluster_membership]: node `{node_id}` is not heal-eligible"
            ));
        }
        node.last_seen_tick = tick;
        node.state = VmClusterNodeState::Active;
        Ok(())
    }

    /// Marks one active or unreachable node as intentionally left.
    pub(crate) fn mark_left(&mut self, node_id: &str, tick: u64) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| format!("error[vm_cluster_membership]: unknown node `{node_id}`"))?;
        if matches!(node.state, VmClusterNodeState::Fenced) {
            return Err(format!(
                "error[vm_cluster_membership]: fenced node `{node_id}` cannot leave"
            ));
        }
        node.last_seen_tick = tick.max(node.last_seen_tick);
        node.state = VmClusterNodeState::Left;
        Ok(())
    }

    /// Fences one known node so it cannot rejoin without a fresh identity.
    pub(crate) fn fence_node(&mut self, node_id: &str, tick: u64) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| format!("error[vm_cluster_membership]: unknown node `{node_id}`"))?;
        node.last_seen_tick = tick.max(node.last_seen_tick);
        node.state = VmClusterNodeState::Fenced;
        Ok(())
    }

    /// Marks active nodes as unreachable when their heartbeat timeout expires.
    pub(crate) fn expire_stale_nodes(&mut self, current_tick: u64) -> Vec<String> {
        let mut expired = Vec::new();
        for node in self.nodes.values_mut() {
            if node.node_id == self.local.node_id() {
                continue;
            }
            if node.state == VmClusterNodeState::Active
                && current_tick.saturating_sub(node.last_seen_tick) > self.heartbeat_timeout_ticks
            {
                node.state = VmClusterNodeState::Unreachable;
                expired.push(node.node_id.clone());
            }
        }
        expired
    }

    /// Removes terminal stale peer snapshots after an explicit retention window.
    pub(crate) fn prune_stale_nodes(
        &mut self,
        current_tick: u64,
        retention_ticks: u64,
    ) -> Result<Vec<String>, String> {
        if retention_ticks == 0 {
            return Err(
                "error[vm_cluster_membership]: stale retention ticks must be non-zero".to_string(),
            );
        }
        let local_node_id = self.local.node_id();
        let removable = self
            .nodes
            .values()
            .filter(|node| {
                node.node_id != local_node_id
                    && matches!(
                        node.state,
                        VmClusterNodeState::Left | VmClusterNodeState::Unreachable
                    )
                    && current_tick.saturating_sub(node.last_seen_tick) > retention_ticks
            })
            .map(|node| node.node_id.clone())
            .collect::<Vec<_>>();
        for node_id in &removable {
            self.nodes.remove(node_id);
            self.node_epochs.remove(node_id);
        }
        Ok(removable)
    }

    /// Returns one node snapshot by node id.
    pub(crate) fn node(&self, node_id: &str) -> Option<&VmClusterNodeSnapshot> {
        self.nodes.get(node_id)
    }

    /// Returns the deterministic membership view ordered by node id.
    pub(crate) fn view(&self) -> Vec<VmClusterNodeSnapshot> {
        self.nodes.values().cloned().collect()
    }
}
