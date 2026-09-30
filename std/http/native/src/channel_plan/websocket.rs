//! Immutable WebSocket route policies, independent of VM and transport state.

use super::WebSocketPlanError;

/// Policy for non-text application payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum BinaryPayloadPolicy {
    /// Reject binary application data on text channels.
    Reject,
}

/// Package-owned WebSocket endpoint descriptor; callbacks are opaque identities.
///
/// Inputs:
/// - Bounded inbound queue size and maximum frame byte size.
///
/// Output:
/// - Route-level policy consumed by channel admission.
///
/// Transformation:
/// - Keeps source-visible endpoint declarations immutable and explicit while
///   live socket state remains owned by the WebSocket session runtime.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WebSocketEndpointPlan<C> {
    max_pending_frames: usize,
    max_frame_bytes: usize,
    binary_payload_policy: BinaryPayloadPolicy,
    callbacks: Option<WebSocketCallbacks<C>>,
    pairing: Option<Box<WebSocketPairing<C>>>,
}

impl<'de, C: serde::Deserialize<'de>> serde::Deserialize<'de> for WebSocketEndpointPlan<C> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(bound(deserialize = "C: serde::Deserialize<'de>"))]
        struct Wire<C> {
            max_pending_frames: usize,
            max_frame_bytes: usize,
            binary_payload_policy: BinaryPayloadPolicy,
            callbacks: Option<WebSocketCallbacks<C>>,
            #[serde(default)]
            pairing: Option<Box<WebSocketPairing<C>>>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let mut plan = Self::new(wire.max_pending_frames, wire.max_frame_bytes)
            .map_err(serde::de::Error::custom)?;
        plan.binary_payload_policy = wire.binary_payload_policy;
        // The first callback set cannot conflict; pairing admission below can.
        plan.callbacks = wire.callbacks;
        if let Some(pairing) = wire.pairing {
            plan = plan
                .with_pairing(*pairing)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(plan)
    }
}

/// Complete callback set for one WebSocket endpoint.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WebSocketCallbacks<C> {
    /// Called after upgrade admission.
    pub open: C,
    /// Called for each admitted inbound frame.
    pub inbound: C,
    /// Called when outbound transport capacity becomes available.
    pub writable: C,
    /// Called during graceful transport close.
    pub close: C,
    /// Called during abrupt scheduler or transport cancellation.
    pub cancellation: C,
}

/// Source-owned payload and callback policy for a two-peer WebSocket session.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(bound(deserialize = "C: serde::Deserialize<'de>"))]
pub struct WebSocketPairing<C> {
    pub waiting: String,
    pub first_matched: String,
    pub second_matched: String,
    pub peer_left: String,
    #[serde(default)]
    pub stateful: bool,
    #[serde(default)]
    pub restoration: Option<WebSocketRestoration<C>>,
    pub inbound: C,
    pub cancellation: C,
}

/// Source-declared identity and callback policy for reclaiming a paired seat.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct WebSocketRestoration<C> {
    pub waiting: C,
    pub peer_left: C,
    pub room_query: String,
    pub player_query: String,
    pub room_prefix: String,
    pub first_player: String,
    pub second_player: String,
    pub retention_ms: u64,
    pub retained_room_capacity: usize,
    pub matched: C,
    pub restored: C,
}

impl<C> WebSocketEndpointPlan<C> {
    /// Transforms callbacks, including pairing recovery, preserving endpoint policy.
    pub fn map_callbacks<D>(self, mut map: impl FnMut(C) -> D) -> WebSocketEndpointPlan<D> {
        WebSocketEndpointPlan {
            max_pending_frames: self.max_pending_frames,
            max_frame_bytes: self.max_frame_bytes,
            binary_payload_policy: self.binary_payload_policy,
            callbacks: self.callbacks.map(|callbacks| WebSocketCallbacks {
                open: map(callbacks.open),
                inbound: map(callbacks.inbound),
                writable: map(callbacks.writable),
                close: map(callbacks.close),
                cancellation: map(callbacks.cancellation),
            }),
            pairing: self
                .pairing
                .map(|pairing| Box::new(pairing.map_callbacks(map))),
        }
    }

    /// Maximum queued frames admitted for one connection.
    pub fn max_pending_frames(&self) -> usize {
        self.max_pending_frames
    }

    /// Maximum decoded bytes admitted for one frame or message.
    pub fn max_frame_bytes(&self) -> usize {
        self.max_frame_bytes
    }

    /// Creates a bounded WebSocket endpoint plan.
    pub fn new(
        max_pending_frames: usize,
        max_frame_bytes: usize,
    ) -> Result<Self, WebSocketPlanError> {
        if max_pending_frames == 0 {
            return Err(WebSocketPlanError::EmptyQueue);
        }
        if max_frame_bytes == 0 {
            return Err(WebSocketPlanError::EmptyFrame);
        }
        Ok(Self {
            max_pending_frames,
            max_frame_bytes,
            binary_payload_policy: BinaryPayloadPolicy::Reject,
            callbacks: None,
            pairing: None,
        })
    }

    /// Attaches one complete callback set to this endpoint.
    pub fn with_callbacks(
        mut self,
        callbacks: WebSocketCallbacks<C>,
    ) -> Result<Self, WebSocketPlanError> {
        if self.pairing.is_some() {
            return Err(WebSocketPlanError::CallbacksConflict);
        }
        if self.callbacks.is_some() {
            return Err(WebSocketPlanError::DuplicateCallbacks);
        }
        self.callbacks = Some(callbacks);
        Ok(self)
    }

    /// Returns the generated callback set retained by this endpoint.
    pub fn callbacks(&self) -> Option<&WebSocketCallbacks<C>> {
        self.callbacks.as_ref()
    }

    /// Attaches one source-declared two-peer delivery policy.
    pub fn with_pairing(
        mut self,
        pairing: WebSocketPairing<C>,
    ) -> Result<Self, WebSocketPlanError> {
        if self.callbacks.is_some() {
            return Err(WebSocketPlanError::PairingConflict);
        }
        if self.pairing.is_some() {
            return Err(WebSocketPlanError::DuplicatePairing);
        }
        self.pairing = Some(Box::new(pairing));
        Ok(self)
    }

    /// Returns the optional source-declared two-peer delivery policy.
    pub fn pairing(&self) -> Option<&WebSocketPairing<C>> {
        self.pairing.as_deref()
    }

    /// Returns the binary payload policy for this endpoint plan.
    pub fn binary_payload_policy(&self) -> BinaryPayloadPolicy {
        self.binary_payload_policy
    }
}

impl<C> WebSocketPairing<C> {
    fn map_callbacks<D>(self, mut map: impl FnMut(C) -> D) -> WebSocketPairing<D> {
        WebSocketPairing {
            waiting: self.waiting,
            first_matched: self.first_matched,
            second_matched: self.second_matched,
            peer_left: self.peer_left,
            stateful: self.stateful,
            restoration: self.restoration.map(|restoration| WebSocketRestoration {
                waiting: map(restoration.waiting),
                peer_left: map(restoration.peer_left),
                room_query: restoration.room_query,
                player_query: restoration.player_query,
                room_prefix: restoration.room_prefix,
                first_player: restoration.first_player,
                second_player: restoration.second_player,
                retention_ms: restoration.retention_ms,
                retained_room_capacity: restoration.retained_room_capacity,
                matched: map(restoration.matched),
                restored: map(restoration.restored),
            }),
            inbound: map(self.inbound),
            cancellation: map(self.cancellation),
        }
    }
}
