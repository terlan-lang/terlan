//! Immutable Cluster sessions over the VM's checked TETF transport state machine.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::runtime::vm::coordination::{
    VmDistributedInboundOutcome, VmDistributedSessionState, VmDistributedTransportFrame,
    VmDistributedTransportSession, VmDistributionDelivery,
};

use super::{text, ClusterResource, ReplValue, VmClusterRuntime, VmRuntimeResult, FRAME, SESSION};

#[path = "cluster_session_values.rs"]
mod values;
use values::{atom, delivery, integer, record, unsigned};

/// A source snapshot and the exact frames emitted along its outbound history.
#[derive(Clone)]
pub(super) struct Session {
    transport: VmDistributedTransportSession,
    sent: BTreeMap<u64, Arc<VmDistributedTransportFrame>>,
}

/// Immutable frame identity shared only with the session snapshots that emitted it.
pub(super) struct Frame(Arc<VmDistributedTransportFrame>);

impl VmClusterRuntime {
    /// Executes transport operations locally; frame production does not perform network I/O.
    pub(super) fn session_call(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
        admitted_atoms: &[String],
    ) -> VmRuntimeResult<ReplValue> {
        match (operation, args) {
            ("std.vm.cluster.open", [local, remote, maximum]) => {
                let maximum = usize::try_from(unsigned(maximum)?)
                    .map_err(|_| "error[vm.cluster.limit]: message limit exceeds host size")?;
                let transport = VmDistributedTransportSession::open(
                    self.profile(owner, local)?.clone(),
                    self.profile(owner, remote)?.clone(),
                    maximum,
                )?;
                self.insert_resource(
                    owner,
                    ClusterResource::Session(Box::new(Session {
                        transport,
                        sent: BTreeMap::new(),
                    })),
                )
            }
            ("std.vm.cluster.send", [session, capability, payload]) => self.send(
                owner,
                session,
                capability,
                payload,
                VmDistributionDelivery::AtMostOnce,
                admitted_atoms,
            ),
            ("std.vm.cluster.send_with", [session, capability, payload, policy]) => self.send(
                owner,
                session,
                capability,
                payload,
                delivery(policy)?,
                admitted_atoms,
            ),
            ("std.vm.cluster.accept", [session, frame]) => {
                let mut updated = self.session(owner, session)?.clone();
                let outcome = updated
                    .transport
                    .accept_inbound_frame(&self.frame(owner, frame)?.0, admitted_atoms)?;
                let outcome = match outcome {
                    VmDistributedInboundOutcome::Accepted => atom("accepted"),
                    VmDistributedInboundOutcome::Duplicate => atom("duplicate"),
                    VmDistributedInboundOutcome::OutOfOrder {
                        expected_message_id,
                    } => record(
                        "OutOfOrder",
                        [("expected_message_id", integer(expected_message_id)?)],
                    ),
                };
                self.session_result(owner, updated, "AcceptResult", "outcome", outcome)
            }
            ("std.vm.cluster.frame_message_id", [frame]) => {
                integer(self.frame(owner, frame)?.0.message_id)
            }
            ("std.vm.cluster.frame_delivery", [frame]) => {
                Ok(values::delivery_value(self.frame(owner, frame)?.0.delivery))
            }
            ("std.vm.cluster.needs_ack", [session, frame]) => {
                let session = self.session(owner, session)?;
                let frame = self.frame(owner, frame)?;
                session.require_emitted(frame)?;
                Ok(ReplValue::Bool(
                    session.transport.needs_ack(frame.0.message_id),
                ))
            }
            ("std.vm.cluster.pending_ack_count", [session]) => {
                integer(self.session(owner, session)?.transport.pending_ack_count() as u64)
            }
            ("std.vm.cluster.acknowledge", [session, frame]) => {
                let mut updated = self.session(owner, session)?.clone();
                let frame = self.frame(owner, frame)?;
                updated.require_emitted(frame)?;
                let id = frame.0.message_id;
                updated.transport.acknowledge(id)?;
                self.session_result(
                    owner,
                    updated,
                    "AcknowledgeResult",
                    "message_id",
                    integer(id)?,
                )
            }
            ("std.vm.cluster.session_state", [session]) => Ok(atom(
                match self.session(owner, session)?.transport.state() {
                    VmDistributedSessionState::Connected => "connected",
                    VmDistributedSessionState::Disconnected => "disconnected",
                },
            )),
            ("std.vm.cluster.disconnect", [session, reason, tick]) => {
                let mut updated = self.session(owner, session)?.clone();
                let event = updated
                    .transport
                    .disconnect(values::reason(reason)?, unsigned(tick)?);
                self.session_result(
                    owner,
                    updated,
                    "DisconnectResult",
                    "event",
                    values::event(event)?,
                )
            }
            ("std.vm.cluster.reconnect", [session, remote, tick]) => {
                let mut updated = self.session(owner, session)?.clone();
                let outcome = updated
                    .transport
                    .reconnect(self.profile(owner, remote)?, unsigned(tick)?)?;
                self.session_result(
                    owner,
                    updated,
                    "ReconnectResult",
                    "outcome",
                    values::reconnect(outcome)?,
                )
            }
            _ => Err(format!(
                "error[vm.cluster.operation]: unsupported cluster operation or arity `{}/{}`",
                operation,
                args.len(),
            )
            .into()),
        }
    }

    /// Encodes before storing either advanced state or a frame, preserving failure atomicity.
    fn send(
        &mut self,
        owner: u64,
        session: &ReplValue,
        capability: &ReplValue,
        payload: &ReplValue,
        policy: VmDistributionDelivery,
        atoms: &[String],
    ) -> VmRuntimeResult<ReplValue> {
        let mut updated = self.session(owner, session)?.clone();
        let frame = Arc::new(updated.transport.encode_message(
            text(capability)?,
            payload.clone(),
            Vec::new(),
            atoms,
            policy,
        )?);
        integer(frame.message_id)?;
        updated.sent.insert(frame.message_id, Arc::clone(&frame));
        let frame = self.insert_resource(owner, ClusterResource::Frame(Frame(frame)))?;
        self.session_result(owner, updated, "SendResult", "frame", frame)
    }

    /// Stores a new immutable source snapshot alongside its typed operation result.
    fn session_result(
        &mut self,
        owner: u64,
        session: Session,
        name: &str,
        field: &str,
        value: ReplValue,
    ) -> VmRuntimeResult<ReplValue> {
        let session = self.insert_resource(owner, ClusterResource::Session(Box::new(session)))?;
        Ok(record(name, [("session", session), (field, value)]))
    }

    /// Validates both the claimed handle identity and the actual stored payload kind.
    fn session(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<&Session> {
        match self.resource(owner, value, SESSION)? {
            ClusterResource::Session(session) => Ok(session),
            _ => Err("error[vm.cluster.kind]: stored resource is not a Session".into()),
        }
    }

    /// Validates frame liveness, actor ownership, and the stored payload kind.
    fn frame(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<&Frame> {
        match self.resource(owner, value, FRAME)? {
            ClusterResource::Frame(frame) => Ok(frame),
            _ => Err("error[vm.cluster.kind]: stored resource is not a Frame".into()),
        }
    }
}

impl Session {
    /// Numeric message ids alone cannot authorize acknowledgements across lanes or forks.
    fn require_emitted(&self, frame: &Frame) -> VmRuntimeResult<()> {
        if self
            .sent
            .get(&frame.0.message_id)
            .is_some_and(|sent| Arc::ptr_eq(sent, &frame.0))
        {
            Ok(())
        } else {
            Err(
                "error[vm.cluster.frame_owner]: frame was not emitted by this session snapshot"
                    .into(),
            )
        }
    }
}
