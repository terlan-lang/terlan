//! Native invocation ownership for one admitted WebSocket connection.

use std::sync::Arc;

use terlan_http_native::channel_plan::WebSocketEndpointPlan;
use terlan_http_native::websocket::session::{InboundQueueInfo, Session};
use terlan_http_native::websocket::Utf8Bytes;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::pure_native::{PureNativeIoWait, PureNativeIoWake};
use crate::runtime::vm::ReplValue;

use super::channel_invocation::{AotChannelCallbackState, AotChannelInvocation};

/// WebSocket lifecycle event currently entering or parked in generated code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::commands::serve) enum AotWebSocketCallbackEvent {
    /// Opening upgrade admission completed.
    Open,
    /// One bounded inbound frame became available.
    Inbound,
    /// A fresh two-peer room was formed.
    PairMatched,
    /// A disconnected paired seat was reclaimed.
    PairRestored,
    /// Resolve source-owned reconnect identity before room admission.
    PairIdentity,
    /// The first peer is waiting for an opponent.
    PairWaiting,
    /// The connected peer's opponent disconnected.
    PairPeerLeft,
    /// Outbound transport capacity became available.
    Writable,
    /// The transport closed gracefully.
    Close,
    /// The scheduler or transport cancelled the connection.
    Cancellation,
}

/// Observable state after one WebSocket callback dispatch or resume.
pub(in crate::commands::serve) type AotWebSocketCallbackState = AotChannelCallbackState;

/// One WebSocket connection bound to a native image generation and callback set.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotWebSocketCallbackSession {
    live: Session<ReplValue>,
    invocation: AotChannelInvocation<AotWebSocketCallbackEvent>,
}

impl AotWebSocketCallbackSession {
    /// Admits one live connection and immediately dispatches its open callback.
    pub(in crate::commands::serve) fn open(
        runtime: Arc<AotHandlerRuntime>,
        module: String,
        live: Session<ReplValue>,
    ) -> Result<Self, String> {
        let invocation = AotChannelInvocation::new("websocket", runtime, module);
        let mut session = Self { live, invocation };
        session.invoke(AotWebSocketCallbackEvent::Open, Vec::new())?;
        Ok(session)
    }

    /// Returns whether the package-owned connection remains open.
    pub(in crate::commands::serve) fn is_open(&self) -> bool {
        self.live.is_open()
    }

    /// Returns the immutable endpoint policy retained by the live connection.
    pub(in crate::commands::serve) fn plan(&self) -> &WebSocketEndpointPlan<ReplValue> {
        self.live.plan()
    }

    /// Returns bounded inbound queue state for transport admission checks.
    pub(in crate::commands::serve) fn inspect(&self) -> InboundQueueInfo {
        self.live.inspect()
    }

    /// Returns callback events that have completed for runtime instrumentation.
    #[cfg(test)]
    pub(in crate::commands::serve) fn completed_events(&self) -> &[AotWebSocketCallbackEvent] {
        self.invocation.completed_events()
    }

    /// Returns whether generated callback work is parked on typed VM I/O.
    pub(in crate::commands::serve) fn is_waiting(&self) -> bool {
        self.invocation.is_waiting()
    }

    /// Queues one decoded frame under the admitted endpoint pressure limits.
    pub(in crate::commands::serve) fn enqueue_inbound(
        &mut self,
        text: Utf8Bytes,
    ) -> Result<(), String> {
        self.inbound_wait()?;
        self.live
            .enqueue_inbound(text)
            .map_err(|error| error.to_string())
    }

    /// Dispatches or wakes generated code with the oldest queued text frame.
    #[cfg(test)]
    pub(in crate::commands::serve) fn dispatch_next_inbound(&mut self) -> Result<bool, String> {
        self.dispatch_next_inbound_output()
            .map(|(dispatched, _)| dispatched)
    }

    /// Dispatches one queued text frame and retains its source callback result.
    pub(in crate::commands::serve) fn dispatch_next_inbound_output(
        &mut self,
    ) -> Result<(bool, Option<ReplValue>), String> {
        let Some((value, wait)) = self.next_inbound_frame()? else {
            return Ok((false, None));
        };
        if let Some(wait) = wait {
            let state = self.resume(wait.wake(ReplValue::String(value)))?;
            Ok((true, completed_value(state)))
        } else {
            let state = self.inbound(value)?;
            Ok((true, completed_value(state)))
        }
    }

    /// Dispatches one paired frame with runtime-owned state and peer metadata.
    pub(in crate::commands::serve) fn dispatch_next_stateful_inbound_output(
        &mut self,
        state: String,
        role: i64,
        first_request: String,
        second_request: String,
    ) -> Result<(bool, Option<ReplValue>), String> {
        let Some((value, wait)) = self.next_inbound_frame()? else {
            return Ok((false, None));
        };
        if let Some(wait) = wait {
            let state = self.resume(wait.wake(ReplValue::String(value)))?;
            Ok((true, completed_value(state)))
        } else {
            let state = self.invoke(
                AotWebSocketCallbackEvent::Inbound,
                vec![
                    ReplValue::String(state),
                    ReplValue::Int(role),
                    ReplValue::String(value),
                    ReplValue::String(first_request),
                    ReplValue::String(second_request),
                ],
            )?;
            Ok((true, completed_value(state)))
        }
    }

    fn inbound_wait(&self) -> Result<Option<PureNativeIoWait>, String> {
        let wait = self.invocation.pending_wait()?;
        if let Some(wait) = &wait {
            if wait.boundary_type() != &TvmBoundaryType::String {
                return Err(format!(
                    "error[serve.websocket.wake_type]: inbound text cannot wake {:?}",
                    wait.boundary_type()
                ));
            }
        }
        Ok(wait)
    }

    fn next_inbound_frame(&mut self) -> Result<Option<(String, Option<PureNativeIoWait>)>, String> {
        if self.live.inspect().pending_frames == 0 {
            return Ok(None);
        }
        let wait = self.inbound_wait()?;
        Ok(self
            .live
            .next_inbound()
            .map(|text| (text.to_string(), wait)))
    }

    /// Resolves reconnect identity using the admitted source callback.
    pub(in crate::commands::serve) async fn dispatch_pair_identity_output(
        &mut self,
        target: String,
    ) -> Result<Option<(String, i64)>, String> {
        let callback = self
            .callback(AotWebSocketCallbackEvent::PairIdentity)
            .cloned()
            .ok_or_else(|| {
                "error[serve.websocket.callback_result]: no reconnect identity callback".to_string()
            })?;
        let value = self
            .invocation
            .invoke_suspendable(
                AotWebSocketCallbackEvent::PairIdentity,
                &callback,
                vec![ReplValue::String(target)],
            )
            .await?;
        terlan_http_native::source_descriptor::restoration_identity(&value)
            .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))
    }

    /// Builds one role-specific payload after a fresh pair is formed.
    pub(in crate::commands::serve) fn dispatch_pair_matched_output(
        &mut self,
        room_id: String,
        role: i64,
        first_request: String,
        second_request: String,
    ) -> Result<String, String> {
        self.invoke_string_callback(
            AotWebSocketCallbackEvent::PairMatched,
            vec![
                ReplValue::String(room_id),
                ReplValue::Int(role),
                ReplValue::String(first_request),
                ReplValue::String(second_request),
            ],
            "paired matched",
        )
    }

    /// Builds the retained role-specific view for one reclaimed seat.
    pub(in crate::commands::serve) fn dispatch_pair_restored_output(
        &mut self,
        room_id: String,
        state: String,
        role: i64,
        first_request: String,
        second_request: String,
    ) -> Result<String, String> {
        self.invoke_string_callback(
            AotWebSocketCallbackEvent::PairRestored,
            vec![
                ReplValue::String(room_id),
                ReplValue::String(state),
                ReplValue::Int(role),
                ReplValue::String(first_request),
                ReplValue::String(second_request),
            ],
            "paired restored",
        )
    }

    /// Builds the typed payload sent while the first peer waits for a match.
    pub(in crate::commands::serve) fn dispatch_pair_waiting_output(
        &mut self,
    ) -> Result<String, String> {
        self.invoke_string_callback(
            AotWebSocketCallbackEvent::PairWaiting,
            Vec::new(),
            "paired waiting payload",
        )
    }

    /// Builds the typed payload sent when a paired peer disconnects.
    pub(in crate::commands::serve) fn dispatch_pair_peer_left_output(
        &mut self,
    ) -> Result<String, String> {
        self.invoke_string_callback(
            AotWebSocketCallbackEvent::PairPeerLeft,
            Vec::new(),
            "paired peer-left payload",
        )
    }

    /// Dispatches one admitted inbound text frame through generated code.
    pub(in crate::commands::serve) fn inbound(
        &mut self,
        value: String,
    ) -> Result<AotWebSocketCallbackState, String> {
        self.invoke(
            AotWebSocketCallbackEvent::Inbound,
            vec![ReplValue::String(value)],
        )
    }

    /// Dispatches one writable transport notification through generated code.
    pub(in crate::commands::serve) fn writable(
        &mut self,
    ) -> Result<AotWebSocketCallbackState, String> {
        self.invoke(AotWebSocketCallbackEvent::Writable, Vec::new())
    }

    /// Dispatches graceful close and ends the live-session lease.
    pub(in crate::commands::serve) fn close(
        &mut self,
    ) -> Result<AotWebSocketCallbackState, String> {
        self.invocation
            .cancel_pending("websocket transport closed".to_string())?;
        let state = self.invoke(AotWebSocketCallbackEvent::Close, Vec::new());
        self.live.close();
        self.invocation
            .finish_terminal(AotWebSocketCallbackEvent::Close, state?)
    }

    /// Cancels parked work, dispatches cancellation, and ends the live lease.
    pub(in crate::commands::serve) fn cancel(
        &mut self,
        reason: String,
    ) -> Result<AotWebSocketCallbackState, String> {
        self.invocation.cancel_pending(reason.clone())?;
        let state = self.invoke(
            AotWebSocketCallbackEvent::Cancellation,
            vec![ReplValue::String(reason)],
        );
        self.live.close();
        self.invocation
            .finish_terminal(AotWebSocketCallbackEvent::Cancellation, state?)
    }

    /// Resumes the exact parked callback from one typed VM I/O wake.
    pub(in crate::commands::serve) fn resume(
        &mut self,
        wake: PureNativeIoWake,
    ) -> Result<AotWebSocketCallbackState, String> {
        self.invocation.resume(wake)
    }

    /// Starts one event using its statically selected callback.
    fn invoke(
        &mut self,
        event: AotWebSocketCallbackEvent,
        args: Vec<ReplValue>,
    ) -> Result<AotWebSocketCallbackState, String> {
        let callback = self.callback(event).cloned();
        self.invocation.invoke(event, callback.as_ref(), args)
    }

    fn invoke_string_callback(
        &mut self,
        event: AotWebSocketCallbackEvent,
        args: Vec<ReplValue>,
        label: &str,
    ) -> Result<String, String> {
        match completed_value(self.invoke(event, args)?) {
            Some(ReplValue::String(payload)) => Ok(payload),
            Some(value) => Err(format!(
                "error[serve.websocket.callback_result]: {label} callback returned {value:?}, expected String"
            )),
            None => Err(format!(
                "error[serve.websocket.callback_result]: {label} callback suspended without producing a payload"
            )),
        }
    }

    /// Selects the static callback assigned to one lifecycle event.
    fn callback(&self, event: AotWebSocketCallbackEvent) -> Option<&ReplValue> {
        if let Some(callbacks) = self.live.plan().callbacks() {
            return Some(match event {
                AotWebSocketCallbackEvent::Open => &callbacks.open,
                AotWebSocketCallbackEvent::Inbound => &callbacks.inbound,
                AotWebSocketCallbackEvent::PairMatched
                | AotWebSocketCallbackEvent::PairIdentity
                | AotWebSocketCallbackEvent::PairRestored
                | AotWebSocketCallbackEvent::PairWaiting
                | AotWebSocketCallbackEvent::PairPeerLeft => return None,
                AotWebSocketCallbackEvent::Writable => &callbacks.writable,
                AotWebSocketCallbackEvent::Close => &callbacks.close,
                AotWebSocketCallbackEvent::Cancellation => &callbacks.cancellation,
            });
        }
        let pairing = self.live.plan().pairing()?;
        match event {
            AotWebSocketCallbackEvent::PairIdentity => pairing
                .restoration
                .as_ref()
                .map(|restoration| &restoration.identity),
            AotWebSocketCallbackEvent::Inbound => Some(&pairing.inbound),
            AotWebSocketCallbackEvent::PairMatched => pairing
                .restoration
                .as_ref()
                .map(|restoration| &restoration.matched),
            AotWebSocketCallbackEvent::PairRestored => pairing
                .restoration
                .as_ref()
                .map(|restoration| &restoration.restored),
            AotWebSocketCallbackEvent::PairWaiting => pairing
                .restoration
                .as_ref()
                .map(|restoration| &restoration.waiting),
            AotWebSocketCallbackEvent::PairPeerLeft => pairing
                .restoration
                .as_ref()
                .map(|restoration| &restoration.peer_left),
            AotWebSocketCallbackEvent::Cancellation => Some(&pairing.cancellation),
            AotWebSocketCallbackEvent::Open
            | AotWebSocketCallbackEvent::Writable
            | AotWebSocketCallbackEvent::Close => None,
        }
    }
}

fn completed_value(state: AotWebSocketCallbackState) -> Option<ReplValue> {
    match state {
        AotChannelCallbackState::Complete(value) => Some(value),
        AotChannelCallbackState::Waiting(_) => None,
    }
}

#[cfg(test)]
#[path = "websocket_invocation_test.rs"]
#[cfg(test)]
mod websocket_invocation_test;

#[cfg(test)]
#[path = "websocket_identity_test.rs"]
mod websocket_identity_test;
