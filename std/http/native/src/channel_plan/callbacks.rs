//! Lifecycle callback selection belongs to the package, not its execution host.

use super::{SseEndpointPlan, WebSocketEndpointPlan};

/// Events dispatched against the source-declared WebSocket callback set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebSocketEvent {
    Open,
    Inbound,
    PairMatched,
    PairRestored,
    PairIdentity,
    PairRoomIdentity,
    PairWaiting,
    PairPeerLeft,
    Writable,
    Close,
    Cancellation,
}

/// Events dispatched against the source-declared SSE callback set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SseEvent {
    Open,
    EventReady,
    KeepAlive,
    Drain,
    Cancellation,
}

impl<C> WebSocketEndpointPlan<C> {
    /// Borrows the selected source callback without interpreting, cloning, or
    /// executing it. An absent or mode-incompatible event returns None; required
    /// admission callbacks must still be checked by the caller.
    pub fn callback(&self, event: WebSocketEvent) -> Option<&C> {
        use WebSocketEvent::*;
        if let Some(callbacks) = self.callbacks() {
            return match event {
                Open => Some(&callbacks.open),
                Inbound => Some(&callbacks.inbound),
                Writable => Some(&callbacks.writable),
                Close => Some(&callbacks.close),
                Cancellation => Some(&callbacks.cancellation),
                PairMatched | PairRestored | PairIdentity | PairRoomIdentity | PairWaiting
                | PairPeerLeft => None,
            };
        }
        let pairing = self.pairing()?;
        let restoration = pairing.restoration.as_ref();
        match event {
            Inbound => Some(&pairing.inbound),
            Cancellation => Some(&pairing.cancellation),
            Open | Writable | Close => None,
            PairMatched => restoration.map(|callbacks| &callbacks.matched),
            PairRestored => restoration.map(|callbacks| &callbacks.restored),
            PairIdentity => restoration.map(|callbacks| &callbacks.identity),
            PairRoomIdentity => restoration.map(|callbacks| &callbacks.room_identity),
            PairWaiting => restoration.map(|callbacks| &callbacks.waiting),
            PairPeerLeft => restoration.map(|callbacks| &callbacks.peer_left),
        }
    }
}

impl<C> SseEndpointPlan<C> {
    /// Borrows a retained callback; an unconfigured endpoint has no callbacks.
    pub fn callback(&self, event: SseEvent) -> Option<&C> {
        let callbacks = self.callbacks()?;
        Some(match event {
            SseEvent::Open => &callbacks.open,
            SseEvent::EventReady => &callbacks.event_ready,
            SseEvent::KeepAlive => &callbacks.keep_alive,
            SseEvent::Drain => &callbacks.drain,
            SseEvent::Cancellation => &callbacks.cancellation,
        })
    }
}

#[cfg(test)]
#[path = "callbacks_test.rs"]
mod tests;
