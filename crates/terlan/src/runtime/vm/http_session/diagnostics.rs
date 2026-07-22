use crate::runtime::vm::process::{VmExitReason, VmProcessId};

pub(super) fn crashed_session_actor_diagnostic(
    session_id: &str,
    actor: VmProcessId,
    reason: &VmExitReason,
) -> String {
    let reason = match reason {
        VmExitReason::Normal => "normal exit".to_string(),
        VmExitReason::Killed => "killed".to_string(),
        VmExitReason::Error(message) => format!("error `{message}`"),
        VmExitReason::ShutdownTimeout { timeout_ms } => {
            format!("shutdown timed out after {timeout_ms} ms")
        }
        VmExitReason::MemoryLimitExceeded {
            requested_bytes,
            previous_bytes,
            projected_bytes,
        } => format!(
            "memory limit exceeded: requested {requested_bytes} bytes from {previous_bytes} bytes, projected {projected_bytes} bytes"
        ),
    };
    format!(
        "HTTP session actor `{session_id}` crashed during request: process {} exited with {reason}",
        actor.as_u64()
    )
}
