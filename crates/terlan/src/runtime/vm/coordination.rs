#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use super::term_format::{encode_tetf_distribution_envelope, TetfDistributionEnvelope, TetfVmRef};
use super::ReplValue;

mod inbound;

use inbound::validate_inbound_frame;

/// Identity and capability metadata for one Terlan VM instance.
///
/// Inputs:
/// - Application, VM, node, cluster, epoch, runtime version, and capabilities.
///
/// Output:
/// - Stable metadata used by future transports and local multi-VM tests.
///
/// Transformation:
/// - Keeps coordination explicit: two VM instances do not trust or route to
///   each other until cluster/runtime and capability checks pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmCoordinationProfile {
    app_id: String,
    vm_id: String,
    node_id: String,
    cluster_id: String,
    epoch: u64,
    runtime_version: String,
    capabilities: BTreeSet<String>,
}

impl VmCoordinationProfile {
    /// Creates a validated coordination profile with deterministic capability ordering.
    pub(crate) fn new(
        app_id: impl Into<String>,
        vm_id: impl Into<String>,
        node_id: impl Into<String>,
        cluster_id: impl Into<String>,
        epoch: u64,
        runtime_version: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, String> {
        let app_id = app_id.into();
        let vm_id = vm_id.into();
        let node_id = node_id.into();
        let cluster_id = cluster_id.into();
        let runtime_version = runtime_version.into();
        let capabilities = capabilities
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<String>>();

        for (field, value) in [
            ("app_id", app_id.as_str()),
            ("vm_id", vm_id.as_str()),
            ("node_id", node_id.as_str()),
            ("cluster_id", cluster_id.as_str()),
            ("runtime_version", runtime_version.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "error[vm_coordination_profile]: `{field}` must not be empty"
                ));
            }
        }
        if epoch == 0 {
            return Err("error[vm_coordination_profile]: `epoch` must be non-zero".to_string());
        }
        if capabilities.iter().any(|value| value.trim().is_empty()) {
            return Err(
                "error[vm_coordination_profile]: capability names must not be empty".to_string(),
            );
        }

        Ok(Self {
            app_id,
            vm_id,
            node_id,
            cluster_id,
            epoch,
            runtime_version,
            capabilities,
        })
    }

    /// Returns whether this VM can coordinate with a peer at the metadata layer.
    pub(crate) fn can_coordinate_with(&self, peer: &Self) -> bool {
        self.cluster_id == peer.cluster_id && self.runtime_version == peer.runtime_version
    }

    /// Returns whether this VM advertises every required capability.
    pub(crate) fn has_capabilities<'a>(&self, required: impl IntoIterator<Item = &'a str>) -> bool {
        required
            .into_iter()
            .all(|capability| self.capabilities.contains(capability))
    }

    /// Returns this VM's epoch.
    pub(crate) const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Returns this VM's application id.
    pub(crate) fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Returns this VM's instance id.
    pub(crate) fn vm_id(&self) -> &str {
        &self.vm_id
    }

    /// Returns this VM's node id.
    pub(crate) fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Returns this profile advanced to the next restart incarnation.
    pub(crate) fn next_epoch(&self) -> Result<Self, String> {
        let epoch = self.epoch.checked_add(1).ok_or_else(|| {
            "error[vm_coordination_profile]: profile epoch cannot advance beyond UInt64".to_string()
        })?;
        Self::new(
            self.app_id.clone(),
            self.vm_id.clone(),
            self.node_id.clone(),
            self.cluster_id.clone(),
            epoch,
            self.runtime_version.clone(),
            self.capabilities.iter().cloned(),
        )
    }
}

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

    /// Restores one validated peer snapshot from an opaque VM descriptor.
    pub(crate) fn restore_peer_snapshot(
        &mut self,
        peer: &VmCoordinationProfile,
        state: VmClusterNodeState,
        last_seen_tick: u64,
        role_tags: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<(), String> {
        if peer.node_id() == self.local.node_id() {
            return Err(
                "error[vm_cluster_membership]: local node cannot be restored as a peer".to_string(),
            );
        }
        if self.nodes.contains_key(peer.node_id()) {
            return Err(format!(
                "error[vm_cluster_membership]: duplicate restored node `{}`",
                peer.node_id()
            ));
        }
        if !self.local.can_coordinate_with(peer) {
            return Err(
                "error[vm_cluster_membership]: incompatible restored VM coordination profile"
                    .to_string(),
            );
        }
        self.nodes.insert(
            peer.node_id().to_string(),
            VmClusterNodeSnapshot::from_profile(peer, state, last_seen_tick, role_tags),
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

/// Monotonic message id allocator for one VM coordination lane.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VmMessageIdAllocator {
    next: u64,
}

impl VmMessageIdAllocator {
    /// Reserves the next monotonic message id without committing it.
    fn reserve(&self) -> Result<u64, String> {
        self.next.checked_add(1).ok_or_else(|| {
            "error[vm_distributed_transport]: message id space is exhausted".to_string()
        })
    }

    /// Commits an id after the complete frame has passed validation.
    fn commit(&mut self, message_id: u64) {
        self.next = message_id;
    }
}

/// Metadata envelope for a future cross-VM coordination message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmCoordinationEnvelope {
    pub(crate) message_id: u64,
    pub(crate) trace_id: String,
    pub(crate) from_app_id: String,
    pub(crate) from_vm_id: String,
    pub(crate) from_node_id: String,
    pub(crate) to_app_id: String,
    pub(crate) to_vm_id: String,
    pub(crate) to_node_id: String,
    pub(crate) capability: String,
    pub(crate) epoch: u64,
}

impl VmCoordinationEnvelope {
    /// Builds a checked envelope between two compatible VM profiles.
    pub(crate) fn new(
        message_id: u64,
        from: &VmCoordinationProfile,
        to: &VmCoordinationProfile,
        capability: impl Into<String>,
    ) -> Result<Self, String> {
        let capability = capability.into();
        if !from.can_coordinate_with(to) {
            return Err(
                "error[vm_coordination]: incompatible VM coordination profiles".to_string(),
            );
        }
        if !to.has_capabilities([capability.as_str()]) {
            return Err(format!(
                "error[vm_coordination]: target VM `{}` lacks capability `{capability}`",
                to.vm_id()
            ));
        }
        Ok(Self {
            message_id,
            trace_id: format!("trace:{}:{}:{}", from.vm_id(), to.vm_id(), message_id),
            from_app_id: from.app_id().to_string(),
            from_vm_id: from.vm_id().to_string(),
            from_node_id: from.node_id().to_string(),
            to_app_id: to.app_id().to_string(),
            to_vm_id: to.vm_id().to_string(),
            to_node_id: to.node_id().to_string(),
            capability,
            epoch: to.epoch(),
        })
    }

    /// Builds the TETF distribution envelope for this coordination message.
    pub(crate) fn to_tetf_distribution_envelope(
        &self,
        refs: Vec<TetfVmRef>,
        payload: ReplValue,
    ) -> TetfDistributionEnvelope {
        TetfDistributionEnvelope::new(
            self.trace_id.clone(),
            self.from_node_id.clone(),
            self.to_node_id.clone(),
            self.epoch,
            refs,
            payload,
        )
    }
}

/// Delivery contract for one VM distributed transport message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributionDelivery {
    AtMostOnce,
    NeedsAck,
}

/// Encoded VM distributed transport frame ready for a backend adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedTransportFrame {
    pub(crate) message_id: u64,
    pub(crate) trace_id: String,
    pub(crate) from_node_id: String,
    pub(crate) to_node_id: String,
    pub(crate) delivery: VmDistributionDelivery,
    pub(crate) bytes: Vec<u8>,
}

/// Inbound delivery classification for one VM distributed transport frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedInboundOutcome {
    Accepted,
    Duplicate,
    OutOfOrder { expected_message_id: u64 },
}

/// Connection lifecycle state for a VM distributed transport session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedSessionState {
    Connected,
    Disconnected,
}

/// Typed reason for a VM distributed transport disconnect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedDisconnectReason {
    LocalClose,
    RemoteClose,
    TransportFailure,
    HeartbeatTimeout,
    Fenced,
}

/// Inspectable disconnect event for a VM distributed transport session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDistributedDisconnectEvent {
    pub(crate) reason: VmDistributedDisconnectReason,
    pub(crate) tick: u64,
    pub(crate) pending_ack_count: usize,
}

/// Outcome for a VM distributed transport reconnect attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDistributedReconnectOutcome {
    AlreadyConnected,
    Reconnected { pending_ack_count: usize },
}

/// In-memory VM distributed transport session between two compatible nodes.
///
/// Inputs:
/// - Local and remote VM coordination profiles.
/// - Maximum encoded message size for this transport lane.
///
/// Output:
/// - Checked transport session that can produce TETF-backed frames and track
///   acknowledgement state for messages requiring confirmation.
///
/// Transformation:
/// - Keeps distributed semantics VM-owned and backend-neutral: this layer
///   creates frames and state transitions, while a later TCP/TLS adapter owns
///   actual network IO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmDistributedTransportSession {
    local: VmCoordinationProfile,
    remote: VmCoordinationProfile,
    max_message_bytes: usize,
    next_message_id: VmMessageIdAllocator,
    pending_acks: BTreeSet<u64>,
    accepted_inbound_message_ids: BTreeSet<u64>,
    next_inbound_message_id: u64,
    state: VmDistributedSessionState,
    last_disconnect: Option<VmDistributedDisconnectEvent>,
    last_reconnect_tick: Option<u64>,
}

/// Serializable state required to resume one distributed transport session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmDistributedTransportSessionSnapshot {
    pub(crate) next_message_id: u64,
    pub(crate) pending_ack_message_ids: Vec<u64>,
    pub(crate) accepted_inbound_message_ids: Vec<u64>,
    pub(crate) next_inbound_message_id: u64,
    pub(crate) state: VmDistributedSessionState,
    pub(crate) last_disconnect: Option<VmDistributedDisconnectEvent>,
    pub(crate) last_reconnect_tick: Option<u64>,
}

impl VmDistributedTransportSession {
    /// Opens a transport session between compatible VM coordination profiles.
    pub(crate) fn open(
        local: VmCoordinationProfile,
        remote: VmCoordinationProfile,
        max_message_bytes: usize,
    ) -> Result<Self, String> {
        if max_message_bytes == 0 {
            return Err(
                "error[vm_distributed_transport]: max message bytes must be non-zero".to_string(),
            );
        }
        if !local.can_coordinate_with(&remote) {
            return Err(
                "error[vm_distributed_transport]: incompatible VM coordination profiles"
                    .to_string(),
            );
        }
        Ok(Self {
            local,
            remote,
            max_message_bytes,
            next_message_id: VmMessageIdAllocator::default(),
            pending_acks: BTreeSet::new(),
            accepted_inbound_message_ids: BTreeSet::new(),
            next_inbound_message_id: 1,
            state: VmDistributedSessionState::Connected,
            last_disconnect: None,
            last_reconnect_tick: None,
        })
    }

    /// Restores a validated transport session from an immutable snapshot.
    pub(crate) fn restore(
        local: VmCoordinationProfile,
        remote: VmCoordinationProfile,
        max_message_bytes: usize,
        snapshot: VmDistributedTransportSessionSnapshot,
    ) -> Result<Self, String> {
        let mut session = Self::open(local, remote, max_message_bytes)?;
        if snapshot.next_inbound_message_id == 0 {
            return Err(
                "error[vm_distributed_transport]: next inbound message id must be non-zero"
                    .to_string(),
            );
        }
        if snapshot
            .pending_ack_message_ids
            .iter()
            .any(|message_id| *message_id == 0 || *message_id > snapshot.next_message_id)
        {
            return Err(
                "error[vm_distributed_transport]: pending acknowledgement id is outside the emitted message range"
                    .to_string(),
            );
        }
        let expected_accepted_count = snapshot.next_inbound_message_id - 1;
        let accepted_count =
            u64::try_from(snapshot.accepted_inbound_message_ids.len()).map_err(|_| {
                "error[vm_distributed_transport]: accepted inbound message history is too large"
                    .to_string()
            })?;
        let accepted_is_contiguous = accepted_count == expected_accepted_count
            && snapshot
                .accepted_inbound_message_ids
                .iter()
                .enumerate()
                .all(|(index, message_id)| {
                    u64::try_from(index)
                        .ok()
                        .and_then(|index| index.checked_add(1))
                        == Some(*message_id)
                });
        if !accepted_is_contiguous {
            return Err(
                "error[vm_distributed_transport]: accepted inbound message ids must form one contiguous prefix"
                    .to_string(),
            );
        }
        if snapshot.state == VmDistributedSessionState::Disconnected
            && snapshot.last_disconnect.is_none()
        {
            return Err(
                "error[vm_distributed_transport]: disconnected session is missing its disconnect event"
                    .to_string(),
            );
        }

        session.next_message_id.next = snapshot.next_message_id;
        session.pending_acks = snapshot.pending_ack_message_ids.into_iter().collect();
        session.accepted_inbound_message_ids =
            snapshot.accepted_inbound_message_ids.into_iter().collect();
        session.next_inbound_message_id = snapshot.next_inbound_message_id;
        session.state = snapshot.state;
        session.last_disconnect = snapshot.last_disconnect;
        session.last_reconnect_tick = snapshot.last_reconnect_tick;
        Ok(session)
    }

    /// Captures the bounded state required to resume this session exactly.
    pub(crate) fn snapshot(&self) -> VmDistributedTransportSessionSnapshot {
        VmDistributedTransportSessionSnapshot {
            next_message_id: self.next_message_id.next,
            pending_ack_message_ids: self.pending_acks.iter().copied().collect(),
            accepted_inbound_message_ids: self
                .accepted_inbound_message_ids
                .iter()
                .copied()
                .collect(),
            next_inbound_message_id: self.next_inbound_message_id,
            state: self.state,
            last_disconnect: self.last_disconnect.clone(),
            last_reconnect_tick: self.last_reconnect_tick,
        }
    }

    /// Encodes one VM distributed message into a bounded TETF transport frame.
    pub(crate) fn encode_message(
        &mut self,
        capability: impl Into<String>,
        payload: ReplValue,
        refs: Vec<TetfVmRef>,
        declared_atoms: &[String],
        delivery: VmDistributionDelivery,
    ) -> Result<VmDistributedTransportFrame, String> {
        self.require_connected()?;
        let message_id = self.next_message_id.reserve()?;
        let envelope =
            VmCoordinationEnvelope::new(message_id, &self.local, &self.remote, capability)?;
        let tetf_envelope = envelope.to_tetf_distribution_envelope(refs, payload);
        let bytes = encode_tetf_distribution_envelope(&tetf_envelope, declared_atoms)?;
        if bytes.len() > self.max_message_bytes {
            return Err(format!(
                "error[vm_distributed_transport]: encoded message `{}` exceeds max message bytes",
                envelope.trace_id
            ));
        }
        self.next_message_id.commit(message_id);
        if delivery == VmDistributionDelivery::NeedsAck {
            self.pending_acks.insert(message_id);
        }
        Ok(VmDistributedTransportFrame {
            message_id,
            trace_id: envelope.trace_id,
            from_node_id: envelope.from_node_id,
            to_node_id: envelope.to_node_id,
            delivery,
            bytes,
        })
    }

    /// Marks a pending acknowledgement as received.
    pub(crate) fn acknowledge(&mut self, message_id: u64) -> Result<(), String> {
        self.require_connected()?;
        if self.pending_acks.remove(&message_id) {
            return Ok(());
        }
        Err(format!(
            "error[vm_distributed_transport]: no pending acknowledgement for message `{message_id}`"
        ))
    }

    /// Returns whether a message id is waiting for acknowledgement.
    pub(crate) fn needs_ack(&self, message_id: u64) -> bool {
        self.pending_acks.contains(&message_id)
    }

    /// Returns the number of messages currently waiting for acknowledgement.
    pub(crate) fn pending_ack_count(&self) -> usize {
        self.pending_acks.len()
    }

    /// Validates and records one inbound frame for this transport session.
    pub(crate) fn accept_inbound_frame(
        &mut self,
        frame: &VmDistributedTransportFrame,
        declared_atoms: &[String],
    ) -> Result<VmDistributedInboundOutcome, String> {
        self.require_connected()?;
        validate_inbound_frame(self, frame, declared_atoms)?;
        if self
            .accepted_inbound_message_ids
            .contains(&frame.message_id)
        {
            return Ok(VmDistributedInboundOutcome::Duplicate);
        }
        if frame.message_id != self.next_inbound_message_id {
            return Ok(VmDistributedInboundOutcome::OutOfOrder {
                expected_message_id: self.next_inbound_message_id,
            });
        }
        self.accepted_inbound_message_ids.insert(frame.message_id);
        while self
            .accepted_inbound_message_ids
            .contains(&self.next_inbound_message_id)
        {
            self.next_inbound_message_id += 1;
        }
        Ok(VmDistributedInboundOutcome::Accepted)
    }

    /// Returns the next inbound message id expected by this session.
    pub(crate) const fn next_inbound_message_id(&self) -> u64 {
        self.next_inbound_message_id
    }

    /// Returns this transport session's current lifecycle state.
    pub(crate) const fn state(&self) -> VmDistributedSessionState {
        self.state
    }

    /// Returns the last recorded disconnect event, if one exists.
    pub(crate) fn last_disconnect(&self) -> Option<&VmDistributedDisconnectEvent> {
        self.last_disconnect.as_ref()
    }

    /// Returns the tick for the last successful reconnect, if one exists.
    pub(crate) const fn last_reconnect_tick(&self) -> Option<u64> {
        self.last_reconnect_tick
    }

    /// Records a typed disconnect and blocks message encode/accept until reconnect.
    pub(crate) fn disconnect(
        &mut self,
        reason: VmDistributedDisconnectReason,
        tick: u64,
    ) -> VmDistributedDisconnectEvent {
        let event = VmDistributedDisconnectEvent {
            reason,
            tick,
            pending_ack_count: self.pending_ack_count(),
        };
        self.state = VmDistributedSessionState::Disconnected;
        self.last_disconnect = Some(event.clone());
        event
    }

    /// Reconnects a disconnected session to the same compatible remote identity.
    pub(crate) fn reconnect(
        &mut self,
        remote: &VmCoordinationProfile,
        tick: u64,
    ) -> Result<VmDistributedReconnectOutcome, String> {
        if self.state == VmDistributedSessionState::Connected {
            return Ok(VmDistributedReconnectOutcome::AlreadyConnected);
        }
        if !self.local.can_coordinate_with(remote) {
            return Err(
                "error[vm_distributed_transport]: incompatible VM coordination profile on reconnect"
                    .to_string(),
            );
        }
        if remote.vm_id() != self.remote.vm_id() || remote.node_id() != self.remote.node_id() {
            return Err(format!(
                "error[vm_distributed_transport]: reconnect profile `{}` does not match session remote `{}`",
                remote.node_id(),
                self.remote.node_id()
            ));
        }
        self.remote = remote.clone();
        self.state = VmDistributedSessionState::Connected;
        self.last_reconnect_tick = Some(tick);
        Ok(VmDistributedReconnectOutcome::Reconnected {
            pending_ack_count: self.pending_ack_count(),
        })
    }

    /// Rejects runtime message operations while the transport is disconnected.
    fn require_connected(&self) -> Result<(), String> {
        if self.state == VmDistributedSessionState::Connected {
            return Ok(());
        }
        Err(
            "error[vm_distributed_transport]: session is disconnected; reconnect before message operations"
                .to_string(),
        )
    }
}

#[cfg(test)]
#[path = "coordination_test.rs"]
mod coordination_test;

#[cfg(test)]
#[path = "coordination_profile_test.rs"]
mod coordination_profile_test;

#[cfg(test)]
#[path = "coordination_distribution_beam_suite_parity_test.rs"]
mod coordination_distribution_beam_suite_parity_test;
