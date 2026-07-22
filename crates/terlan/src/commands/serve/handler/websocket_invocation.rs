#![cfg_attr(not(test), allow(dead_code))]

//! Native invocation ownership for one admitted WebSocket connection.

use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::native_callable::VmNativeCallableRef;
use crate::runtime::vm::pure_native::PureNativeIoWake;
use crate::runtime::vm::websocket::{
    VmWebSocketCallbackPlan, VmWebSocketEndpointPlan, VmWebSocketFrame,
    VmWebSocketInboundQueueInfo, VmWebSocketLiveSession,
};
use crate::runtime::vm::ReplValue;

use super::channel_invocation::{AotChannelCallbackState, AotChannelInvocation};

/// WebSocket lifecycle event currently entering or parked in generated code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::commands::serve) enum AotWebSocketCallbackEvent {
    /// Opening upgrade admission completed.
    Open,
    /// One bounded inbound frame became available.
    Inbound,
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
    live: VmWebSocketLiveSession,
    callbacks: Option<VmWebSocketCallbackPlan>,
    invocation: AotChannelInvocation<AotWebSocketCallbackEvent>,
}

impl AotWebSocketCallbackSession {
    /// Admits one live connection and immediately dispatches its open callback.
    pub(in crate::commands::serve) fn open(
        runtime: Arc<AotHandlerRuntime>,
        module: String,
        live: VmWebSocketLiveSession,
    ) -> Result<Self, String> {
        let callbacks = live.plan().callbacks().cloned();
        let invocation = AotChannelInvocation::new("websocket", runtime, module);
        let mut session = Self {
            live,
            callbacks,
            invocation,
        };
        session.invoke(AotWebSocketCallbackEvent::Open, Vec::new())?;
        Ok(session)
    }

    /// Returns whether the underlying VM-owned connection remains open.
    pub(in crate::commands::serve) fn is_open(&self) -> bool {
        self.live.is_open()
    }

    /// Returns the immutable endpoint policy retained by the live connection.
    pub(in crate::commands::serve) fn plan(&self) -> &VmWebSocketEndpointPlan {
        self.live.plan()
    }

    /// Returns bounded inbound queue state for transport admission checks.
    pub(in crate::commands::serve) fn inspect(&self) -> VmWebSocketInboundQueueInfo {
        self.live.inspect()
    }

    /// Returns callback events that have completed for runtime instrumentation.
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
        frame: VmWebSocketFrame,
    ) -> Result<(), String> {
        self.live.enqueue_inbound(frame)
    }

    /// Dispatches or wakes generated code with the oldest queued text frame.
    pub(in crate::commands::serve) fn dispatch_next_inbound(&mut self) -> Result<bool, String> {
        let Some(frame) = self.live.next_inbound() else {
            return Ok(false);
        };
        let VmWebSocketFrame::Text(value) = frame else {
            return Err(
                "error[serve.websocket.callback_frame]: only admitted text data frames enter the source callback"
                    .to_string(),
            );
        };
        if let Some(wait) = self.invocation.pending_wait()? {
            if wait.boundary_type() != &TvmBoundaryType::String {
                return Err(format!(
                    "error[serve.websocket.wake_type]: inbound text cannot wake {:?}",
                    wait.boundary_type()
                ));
            }
            self.resume(wait.wake(ReplValue::String(value)))?;
        } else {
            self.inbound(VmWebSocketFrame::Text(value))?;
        }
        Ok(true)
    }

    /// Dispatches one admitted inbound text frame through generated code.
    pub(in crate::commands::serve) fn inbound(
        &mut self,
        frame: VmWebSocketFrame,
    ) -> Result<AotWebSocketCallbackState, String> {
        let VmWebSocketFrame::Text(value) = frame else {
            return Err(
                "error[serve.websocket.callback_frame]: only admitted text data frames enter the source callback"
                    .to_string(),
            );
        };
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

    /// Selects the static callback assigned to one lifecycle event.
    fn callback(&self, event: AotWebSocketCallbackEvent) -> Option<&VmNativeCallableRef> {
        let callbacks = self.callbacks.as_ref()?;
        Some(match event {
            AotWebSocketCallbackEvent::Open => &callbacks.open,
            AotWebSocketCallbackEvent::Inbound => &callbacks.inbound,
            AotWebSocketCallbackEvent::Writable => &callbacks.writable,
            AotWebSocketCallbackEvent::Close => &callbacks.close,
            AotWebSocketCallbackEvent::Cancellation => &callbacks.cancellation,
        })
    }
}

#[cfg(test)]
#[path = "websocket_invocation_test.rs"]
mod websocket_invocation_test;
