//! Checked source-level transport enum and record conversions.

use super::{ReplValue, VmRuntimeResult};
use crate::runtime::vm::coordination::{
    VmDistributedDisconnectEvent, VmDistributedDisconnectReason, VmDistributedReconnectOutcome,
    VmDistributionDelivery,
};

/// Reads a nonnegative source tick or size without wrapping signed values.
pub(super) fn unsigned(value: &ReplValue) -> VmRuntimeResult<u64> {
    match value {
        ReplValue::Int(value) => u64::try_from(*value)
            .map_err(|_| "error[vm.cluster.integer]: expected nonnegative Int".into()),
        _ => Err("error[vm.cluster.integer]: expected Int".into()),
    }
}

/// Rejects transport counters that cannot be represented by a source Int.
pub(super) fn integer(value: u64) -> VmRuntimeResult<ReplValue> {
    i64::try_from(value)
        .map(ReplValue::Int)
        .map_err(|_| "error[vm.cluster.integer]: counter exceeds Terlan Int".into())
}

/// Constructs the canonical runtime representation of a nullary source type.
pub(super) fn atom(name: &str) -> ReplValue {
    ReplValue::Atom(name.into())
}

/// Constructs one named result using the existing managed-record ABI.
pub(super) fn record<const N: usize>(name: &str, fields: [(&str, ReplValue); N]) -> ReplValue {
    ReplValue::Record {
        name: name.into(),
        fields: fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    }
}

/// Decodes only members of the closed source delivery domain.
pub(super) fn delivery(value: &ReplValue) -> VmRuntimeResult<VmDistributionDelivery> {
    match value {
        ReplValue::Atom(value) if value == "at_most_once" => Ok(VmDistributionDelivery::AtMostOnce),
        ReplValue::Atom(value) if value == "needs_ack" => Ok(VmDistributionDelivery::NeedsAck),
        _ => Err("error[vm.cluster.delivery]: expected AtMostOnce or NeedsAck".into()),
    }
}

/// Encodes the delivery contract without changing its acknowledgement semantics.
pub(super) fn delivery_value(value: VmDistributionDelivery) -> ReplValue {
    atom(match value {
        VmDistributionDelivery::AtMostOnce => "at_most_once",
        VmDistributionDelivery::NeedsAck => "needs_ack",
    })
}

/// Preserves all five source disconnect reasons as distinct VM values.
pub(super) fn reason(value: &ReplValue) -> VmRuntimeResult<VmDistributedDisconnectReason> {
    use VmDistributedDisconnectReason as Reason;
    match value {
        ReplValue::Atom(value) => match value.as_str() {
            "local_close" => Ok(Reason::LocalClose),
            "remote_close" => Ok(Reason::RemoteClose),
            "transport_failure" => Ok(Reason::TransportFailure),
            "heartbeat_timeout" => Ok(Reason::HeartbeatTimeout),
            "session_fenced" => Ok(Reason::SessionFenced),
            _ => Err("error[vm.cluster.reason]: unknown disconnect reason".into()),
        },
        _ => Err("error[vm.cluster.reason]: expected a typed disconnect reason".into()),
    }
}

/// Projects an immutable disconnect event into its declared source fields.
pub(super) fn event(event: VmDistributedDisconnectEvent) -> VmRuntimeResult<ReplValue> {
    use VmDistributedDisconnectReason as Reason;
    let reason = atom(match event.reason {
        Reason::LocalClose => "local_close",
        Reason::RemoteClose => "remote_close",
        Reason::TransportFailure => "transport_failure",
        Reason::HeartbeatTimeout => "heartbeat_timeout",
        Reason::SessionFenced => "session_fenced",
    });
    Ok(record(
        "DisconnectEvent",
        [
            ("reason", reason),
            ("tick", integer(event.tick)?),
            (
                "pending_ack_count",
                integer(event.pending_ack_count as u64)?,
            ),
        ],
    ))
}

/// Projects both reconnect outcomes without erasing the pending count.
pub(super) fn reconnect(outcome: VmDistributedReconnectOutcome) -> VmRuntimeResult<ReplValue> {
    Ok(match outcome {
        VmDistributedReconnectOutcome::AlreadyConnected => atom("already_connected"),
        VmDistributedReconnectOutcome::Reconnected { pending_ack_count } => record(
            "Reconnected",
            [("pending_ack_count", integer(pending_ack_count as u64)?)],
        ),
    })
}
