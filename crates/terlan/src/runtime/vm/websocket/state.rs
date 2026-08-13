use super::*;

/// VM-owned WebSocket upgrade response metadata.
///
/// Inputs:
/// - Created from a validated `Sec-WebSocket-Key` opening-handshake header.
///
/// Output:
/// - HTTP status and response headers needed to switch protocols.
///
/// Transformation:
/// - Keeps WebSocket handshake planning inside the VM runtime while leaving
///   frame scheduling as a later transport slice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmWebSocketUpgradeResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
}

/// VM-owned accepted WebSocket upgrade handoff.
///
/// Inputs:
/// - Validated VM TCP stream, WebSocket endpoint plan, and opening-handshake
///   key.
///
/// Output:
/// - Bound WebSocket session plus protocol-switch response metadata.
///
/// Transformation:
/// - Gives HTTP upgrade handling one atomic VM-owned handoff that validates
///   transport state before any actor sees a live WebSocket session.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmWebSocketAcceptedUpgrade {
    pub(crate) session: VmWebSocketSessionId,
    pub(crate) response: VmWebSocketUpgradeResponse,
    pub(crate) endpoint: VmWebSocketEndpointPlan,
}

/// VM-owned WebSocket control-frame payload.
///
/// Inputs:
/// - Produced by maintained WebSocket frame parsing.
///
/// Output:
/// - Typed control operation for VM scheduling and session lifecycle code.
///
/// Transformation:
/// - Prevents callers from depending on tungstenite message internals while
///   keeping protocol parsing delegated to maintained Rust code.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum VmWebSocketControlFrame {
    #[cfg(test)]
    Ping(Vec<u8>),
    #[cfg(test)]
    Pong(Vec<u8>),
    Close,
}

/// VM-owned WebSocket frame event.
///
/// Inputs:
/// - Produced by maintained WebSocket frame parsing over VM-owned stream bytes.
///
/// Output:
/// - Typed data or control event for WebSocket actors.
///
/// Transformation:
/// - Gives scheduler and actor code one receive surface for mixed frame
///   streams without exposing tungstenite messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmWebSocketFrame {
    #[cfg(test)]
    Text(String),
    #[cfg(test)]
    Control(VmWebSocketControlFrame),
}

/// VM-owned WebSocket binary payload policy.
///
/// Inputs:
/// - Endpoint-level declaration for non-text data frames.
///
/// Output:
/// - Explicit binary-frame handling policy.
///
/// Transformation:
/// - Prevents binary payload behavior from being an accidental decoder detail
///   while keeping the first channel surface text/control focused.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum VmWebSocketBinaryPayloadPolicy {
    Reject,
}

impl VmWebSocketFrame {
    #[cfg(test)]
    pub(crate) fn payload_len(&self) -> usize {
        match self {
            Self::Text(value) => value.len(),
            Self::Control(VmWebSocketControlFrame::Ping(value))
            | Self::Control(VmWebSocketControlFrame::Pong(value)) => value.len(),
            Self::Control(VmWebSocketControlFrame::Close) => 0,
        }
    }
}

/// VM-owned WebSocket endpoint plan.
///
/// Inputs:
/// - Bounded inbound queue size and maximum frame byte size.
///
/// Output:
/// - Route-level WebSocket policy consumed by VM HTTP lowering.
///
/// Transformation:
/// - Keeps source-visible endpoint declarations immutable and explicit while
///   live socket state remains owned by the WebSocket session runtime.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct VmWebSocketEndpointPlan {
    pub(crate) max_pending_frames: usize,
    pub(crate) max_frame_bytes: usize,
    pub(crate) binary_payload_policy: VmWebSocketBinaryPayloadPolicy,
    pub(crate) callbacks: Option<VmWebSocketCallbackPlan>,
}

/// Complete static callback set for one generated WebSocket endpoint.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct VmWebSocketCallbackPlan {
    /// Called after upgrade admission.
    pub(crate) open: VmNativeCallableRef,
    /// Called for each admitted inbound frame.
    pub(crate) inbound: VmNativeCallableRef,
    /// Called when outbound transport capacity becomes available.
    pub(crate) writable: VmNativeCallableRef,
    /// Called during graceful transport close.
    pub(crate) close: VmNativeCallableRef,
    /// Called during abrupt scheduler or transport cancellation.
    pub(crate) cancellation: VmNativeCallableRef,
}

impl VmWebSocketEndpointPlan {
    /// Creates a bounded WebSocket endpoint plan.
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    pub(crate) fn new(max_pending_frames: usize, max_frame_bytes: usize) -> Result<Self, String> {
        if max_pending_frames == 0 {
            return Err(
                "error[vm_websocket_endpoint]: max_pending_frames must be greater than 0"
                    .to_string(),
            );
        }
        if max_frame_bytes == 0 {
            return Err(
                "error[vm_websocket_endpoint]: max_frame_bytes must be greater than 0".to_string(),
            );
        }
        Ok(Self {
            max_pending_frames,
            max_frame_bytes,
            binary_payload_policy: VmWebSocketBinaryPayloadPolicy::Reject,
            callbacks: None,
        })
    }

    /// Attaches one complete closure-free callback set to this endpoint.
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    pub(crate) fn with_callbacks(
        mut self,
        callbacks: VmWebSocketCallbackPlan,
    ) -> Result<Self, String> {
        if self.callbacks.is_some() {
            return Err("error[vm_websocket_endpoint]: callbacks already configured".to_string());
        }
        self.callbacks = Some(callbacks);
        Ok(self)
    }

    /// Returns the generated callback set retained by this endpoint.
    pub(crate) fn callbacks(&self) -> Option<&VmWebSocketCallbackPlan> {
        self.callbacks.as_ref()
    }

    /// Returns the binary payload policy for this endpoint plan.
    #[cfg(test)]
    pub(crate) fn binary_payload_policy(&self) -> VmWebSocketBinaryPayloadPolicy {
        self.binary_payload_policy
    }

    /// Opens a bounded VM-owned inbound frame queue for one endpoint session.
    pub(crate) fn open_inbound_queue(&self) -> VmWebSocketInboundQueue {
        VmWebSocketInboundQueue::new(self.max_pending_frames, self.max_frame_bytes)
    }
}

/// Inspectable VM WebSocket session state.
///
/// Inputs:
/// - Snapshot produced from a VM-owned WebSocket session.
///
/// Output:
/// - Runtime-visible lifecycle and traffic counters.
///
/// Transformation:
/// - Exposes scheduling/debugging state without exposing tungstenite sockets
///   or host stream internals.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmWebSocketSessionInfo {
    pub(crate) stream: VmTcpStream,
    pub(crate) open: bool,
    pub(crate) frames_sent: usize,
    pub(crate) frames_received: usize,
    pub(crate) bytes_sent: usize,
    pub(crate) bytes_received: usize,
}

/// Inspectable VM WebSocket runtime aggregate state.
///
/// Inputs:
/// - Snapshot produced from the WebSocket runtime registry.
///
/// Output:
/// - Aggregate lifecycle and traffic counters across tracked sessions.
///
/// Transformation:
/// - Exposes debugger/status data without exposing registry internals.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmWebSocketRuntimeInfo {
    pub(crate) session_count: usize,
    pub(crate) open_sessions: usize,
    pub(crate) closed_sessions: usize,
    pub(crate) frames_sent: usize,
    pub(crate) frames_received: usize,
    pub(crate) bytes_sent: usize,
    pub(crate) bytes_received: usize,
}

/// Per-session outcome for best-effort WebSocket shutdown.
///
/// Inputs:
/// - Produced while closing a caller-provided session list.
///
/// Output:
/// - The attempted session handle and either its final state or a stable
///   diagnostic explaining why that handle was not closed.
///
/// Transformation:
/// - Lets supervisors and actor cleanup collect partial shutdown results
///   without weakening the strict all-or-nothing selected shutdown API.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmWebSocketCloseOutcome {
    pub(crate) session: VmWebSocketSessionId,
    pub(crate) result: Result<VmWebSocketSessionInfo, String>,
}

/// Scheduler-facing WebSocket termination reason.
///
/// Inputs:
/// - Runtime timeout or actor cancellation decision.
///
/// Output:
/// - Stable reason carried with the final session state.
///
/// Transformation:
/// - Lets supervision and debugging distinguish graceful timeout cleanup from
///   abrupt cancellation without inferring from TCP stream state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmWebSocketTerminationReason {
    Timeout,
    Cancelled,
}

/// Result of scheduler-facing WebSocket termination.
///
/// Inputs:
/// - One WebSocket session selected by scheduler timeout/cancellation policy.
///
/// Output:
/// - Session handle, explicit reason, and final inspectable state.
///
/// Transformation:
/// - Preserves lifecycle context after the live session has been removed from
///   the runtime registry.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmWebSocketTermination {
    pub(crate) session: VmWebSocketSessionId,
    pub(crate) reason: VmWebSocketTerminationReason,
    pub(crate) info: VmWebSocketSessionInfo,
}

/// Per-session outcome for best-effort WebSocket fan-out.
///
/// Inputs:
/// - Produced while sending one frame to a caller-provided session list.
///
/// Output:
/// - The attempted session handle and either byte count written or a stable
///   diagnostic explaining why that handle did not receive the frame.
///
/// Transformation:
/// - Lets room actors broadcast through valid live sessions while preserving
///   diagnostics for stale, duplicate, or closed session references.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmWebSocketSendOutcome {
    pub(crate) session: VmWebSocketSessionId,
    pub(crate) result: Result<usize, String>,
}

/// VM-owned WebSocket session handle.
///
/// Inputs: opaque runtime id. Output: stable WebSocket session handle.
/// Transformation: keeps higher-level VM actors away from registry internals.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg(test)]
pub(crate) struct VmWebSocketSessionId {
    pub(super) id: u64,
}

/// VM-owned WebSocket session registry.
///
/// Inputs:
/// - Accepted VM TCP stream handles and WebSocket frame operations.
///
/// Output:
/// - Stable WebSocket session handles and inspectable session snapshots.
///
/// Transformation:
/// - Centralizes WebSocket session ownership in the VM so production handlers
///   do not store ad hoc session maps in HTTP or actor code.
#[derive(Debug, Default)]
#[cfg(test)]
pub(crate) struct VmWebSocketRuntime {
    next_session: u64,
    pub(super) sessions: HashMap<u64, VmWebSocketSession>,
}

#[path = "state/framing.rs"]
mod framing;
#[path = "state/runtime.rs"]
mod runtime;

#[cfg(test)]
pub(crate) use framing::*;
pub(crate) use runtime::*;
