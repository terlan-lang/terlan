use std::collections::BTreeSet;

use super::term_format::{
    encode_tetf_distribution_envelope_bounded, TetfDistributionEnvelope, TetfVmRef,
};
use super::ReplValue;

mod inbound;

use inbound::validate_inbound_frame;

pub(crate) use super::coordination_profile::VmCoordinationProfile;

#[cfg(test)]
pub(crate) use super::coordination_membership::{
    VmClusterMembership, VmClusterNodeSnapshot, VmClusterNodeState,
};

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
    SessionFenced,
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
#[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
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
        let bytes = encode_tetf_distribution_envelope_bounded(
            &tetf_envelope,
            declared_atoms,
            self.max_message_bytes,
        ).map_err(|error| {
            if error.starts_with("error[tetf_size]") {
                format!("error[vm_distributed_transport]: encoded message `{}` exceeds max message bytes", envelope.trace_id)
            } else {
                error.to_string()
            }
        })?;
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
    #[cfg(test)]
    pub(crate) const fn next_inbound_message_id(&self) -> u64 {
        self.next_inbound_message_id
    }

    /// Returns this transport session's current lifecycle state.
    pub(crate) const fn state(&self) -> VmDistributedSessionState {
        self.state
    }

    /// Returns the last recorded disconnect event, if one exists.
    #[cfg(test)]
    pub(crate) fn last_disconnect(&self) -> Option<&VmDistributedDisconnectEvent> {
        self.last_disconnect.as_ref()
    }

    /// Returns the tick for the last successful reconnect, if one exists.
    #[cfg(test)]
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
        if !self.local.can_coordinate_with(remote) {
            return Err(
                "error[vm_distributed_transport]: incompatible VM coordination profile on reconnect"
                    .to_string(),
            );
        }
        if remote.app_id() != self.remote.app_id()
            || remote.vm_id() != self.remote.vm_id()
            || remote.node_id() != self.remote.node_id()
            || remote.epoch() < self.remote.epoch()
        {
            return Err(format!(
                "error[vm_distributed_transport]: reconnect profile `{}` does not match session remote `{}`",
                remote.node_id(),
                self.remote.node_id()
            ));
        }
        if self
            .last_disconnect
            .as_ref()
            .is_some_and(|event| tick < event.tick)
            || self
                .last_reconnect_tick
                .is_some_and(|previous| tick < previous)
        {
            return Err(
                "error[vm_distributed_transport]: reconnect tick moved backwards".to_string(),
            );
        }
        if self.state == VmDistributedSessionState::Connected {
            return Ok(VmDistributedReconnectOutcome::AlreadyConnected);
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
#[cfg(test)]
mod coordination_test;

#[cfg(test)]
#[path = "coordination_profile_test.rs"]
#[cfg(test)]
mod coordination_profile_test;

#[cfg(test)]
#[path = "coordination_distribution_beam_suite_parity_test.rs"]
#[cfg(test)]
mod coordination_distribution_beam_suite_parity_test;
