//! Package-owned channel descriptors. They allocate no sockets, tasks, or queues.

mod sse;
mod websocket;

pub use sse::{SseCallbacks, SseEndpointPlan};
pub use websocket::{
    BinaryPayloadPolicy, WebSocketCallbacks, WebSocketEndpointPlan, WebSocketPairing,
    WebSocketRestoration,
};

/// SSE policy and stream failures shared by package codecs and host adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SseError {
    Codec(String),
    Closed,
    BackpressureExceeded,
    InvalidEventName,
    InvalidRetry,
    InvalidKeepAlive,
    HeartbeatTimedOut,
    InvalidReconnectToken,
    StaleReconnectToken,
    InvalidProtocolAssetHash,
    StaleProtocolAssetHash,
    DomPatchBackpressureExceeded,
    EventTooLarge,
    CallbacksAlreadyConfigured,
}

impl std::fmt::Display for SseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SseError {}

/// Invalid WebSocket route policy, before any transport state is allocated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebSocketPlanError {
    EmptyQueue,
    EmptyFrame,
    CallbacksConflict,
    DuplicateCallbacks,
    PairingConflict,
    DuplicatePairing,
}

impl std::fmt::Display for WebSocketPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = match self {
            Self::EmptyQueue => "max_pending_frames must be greater than 0",
            Self::EmptyFrame => "max_frame_bytes must be greater than 0",
            Self::CallbacksConflict => "callbacks conflict with pairing",
            Self::DuplicateCallbacks => "callbacks already configured",
            Self::PairingConflict => "pairing conflicts with callbacks",
            Self::DuplicatePairing => "pairing already configured",
        };
        write!(f, "error[vm_websocket_endpoint]: {reason}")
    }
}

impl std::error::Error for WebSocketPlanError {}

#[cfg(test)]
#[path = "channel_plan_test.rs"]
mod tests;

#[cfg(test)]
#[path = "channel_plan_mapping_test.rs"]
mod mapping_tests;
