use super::{VmDistributedTransportFrame, VmDistributedTransportSession};
use crate::runtime::vm::term_format::decode_tetf_distribution_envelope;

/// Validates one inbound frame before delivery state can change.
///
/// Validation binds the encoded envelope to the session route, destination
/// epoch, message identity, receiver atom manifest, and byte budget. Parsing
/// also enforces the canonical TETF value contract.
pub(super) fn validate_inbound_frame(
    session: &VmDistributedTransportSession,
    frame: &VmDistributedTransportFrame,
    declared_atoms: &[String],
) -> Result<(), String> {
    if frame.from_node_id != session.remote.node_id() || frame.to_node_id != session.local.node_id()
    {
        return Err(format!(
            "error[vm_distributed_transport]: frame `{}` is not addressed to this session",
            frame.trace_id
        ));
    }
    if frame.bytes.len() > session.max_message_bytes {
        return Err(format!(
            "error[vm_distributed_transport]: inbound frame `{}` exceeds max message bytes",
            frame.trace_id
        ));
    }

    let envelope = decode_tetf_distribution_envelope(&frame.bytes, declared_atoms)?;
    let expected_trace_id = format!(
        "trace:{}:{}:{}",
        session.remote.vm_id(),
        session.local.vm_id(),
        frame.message_id
    );
    if frame.trace_id == expected_trace_id
        && envelope.trace_id == frame.trace_id
        && envelope.from_node_id == frame.from_node_id
        && envelope.to_node_id == frame.to_node_id
        && envelope.epoch == session.local.epoch()
    {
        return Ok(());
    }
    Err(format!(
        "error[vm_distributed_transport]: frame `{}` metadata does not match its TETF envelope or session",
        frame.trace_id
    ))
}
