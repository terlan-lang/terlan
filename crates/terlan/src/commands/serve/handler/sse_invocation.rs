#![cfg_attr(not(test), allow(dead_code))]

//! Native invocation ownership for one admitted SSE stream.

use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::native_callable::VmNativeCallableRef;
use crate::runtime::vm::pure_native::PureNativeIoWake;
use crate::runtime::vm::sse::{
    VmSseCallbackPlan, VmSseEndpointPlan, VmSseEvent, VmSseLiveSession, VmSseStreamInfo,
};
use crate::runtime::vm::ReplValue;

use super::channel_invocation::{AotChannelCallbackState, AotChannelInvocation};

/// SSE lifecycle event currently entering or parked in generated code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::commands::serve) enum AotSseCallbackEvent {
    /// Stream admission completed.
    Open,
    /// One application event became ready for the stream.
    EventReady,
    /// The VM emitted a keep-alive comment.
    KeepAlive,
    /// The stream began graceful drain and close.
    Drain,
    /// The scheduler or transport cancelled the stream.
    Cancellation,
}

/// Observable state after one SSE callback dispatch or resume.
pub(in crate::commands::serve) type AotSseCallbackState = AotChannelCallbackState;

/// One SSE stream bound to a native image generation and callback set.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotSseCallbackSession {
    live: VmSseLiveSession,
    callbacks: Option<VmSseCallbackPlan>,
    invocation: AotChannelInvocation<AotSseCallbackEvent>,
}

impl AotSseCallbackSession {
    /// Admits one live stream and immediately dispatches its open callback.
    pub(in crate::commands::serve) fn open(
        runtime: Arc<AotHandlerRuntime>,
        module: String,
        live: VmSseLiveSession,
    ) -> Result<Self, String> {
        let callbacks = live.plan().callbacks().cloned();
        let invocation = AotChannelInvocation::new("sse", runtime, module);
        let mut session = Self {
            live,
            callbacks,
            invocation,
        };
        session.invoke(AotSseCallbackEvent::Open, Vec::new())?;
        Ok(session)
    }

    /// Returns whether the underlying VM-owned stream remains open.
    pub(in crate::commands::serve) fn is_open(&self) -> bool {
        self.live.is_open()
    }

    /// Returns the immutable endpoint policy retained by the live stream.
    pub(in crate::commands::serve) fn plan(&self) -> &VmSseEndpointPlan {
        self.live.plan()
    }

    /// Returns bounded queue state for transport admission checks.
    pub(in crate::commands::serve) fn inspect(&self) -> VmSseStreamInfo {
        self.live.inspect()
    }

    /// Returns callback events that completed for runtime instrumentation.
    pub(in crate::commands::serve) fn completed_events(&self) -> &[AotSseCallbackEvent] {
        self.invocation.completed_events()
    }

    /// Returns whether generated callback work is parked on typed VM I/O.
    pub(in crate::commands::serve) fn is_waiting(&self) -> bool {
        self.invocation.is_waiting()
    }

    /// Queues one data event and dispatches or wakes its generated callback.
    pub(in crate::commands::serve) fn enqueue_event(
        &mut self,
        data: String,
    ) -> Result<AotSseCallbackState, String> {
        self.live
            .enqueue(VmSseEvent::data(data.clone()))
            .map_err(|error| format!("error[serve.sse.queue]: {error:?}"))?;
        if let Some(wait) = self.invocation.pending_wait()? {
            if wait.boundary_type() != &TvmBoundaryType::String {
                return Err(format!(
                    "error[serve.sse.wake_type]: event data cannot wake {:?}",
                    wait.boundary_type()
                ));
            }
            self.resume(wait.wake(ReplValue::String(data)))
        } else {
            self.event_ready(data)
        }
    }

    /// Encodes and removes the oldest event ready for HTTP stream transport.
    pub(in crate::commands::serve) fn flush_next_event(
        &mut self,
    ) -> Result<Option<Vec<u8>>, String> {
        self.live
            .flush_next()
            .map_err(|error| format!("error[serve.sse.queue]: {error:?}"))
    }

    /// Dispatches one ready application event through generated code.
    pub(in crate::commands::serve) fn event_ready(
        &mut self,
        data: String,
    ) -> Result<AotSseCallbackState, String> {
        self.invoke(
            AotSseCallbackEvent::EventReady,
            vec![ReplValue::String(data)],
        )
    }

    /// Dispatches one VM keep-alive notification through generated code.
    pub(in crate::commands::serve) fn keep_alive(&mut self) -> Result<AotSseCallbackState, String> {
        self.invoke(AotSseCallbackEvent::KeepAlive, Vec::new())
    }

    /// Dispatches graceful drain and ends the live-session lease.
    pub(in crate::commands::serve) fn drain(&mut self) -> Result<AotSseCallbackState, String> {
        self.invocation
            .cancel_pending("SSE transport began graceful drain".to_string())?;
        let state = self.invoke(AotSseCallbackEvent::Drain, Vec::new());
        self.live.close();
        self.invocation
            .finish_terminal(AotSseCallbackEvent::Drain, state?)
    }

    /// Cancels parked work, dispatches cancellation, and ends the live lease.
    pub(in crate::commands::serve) fn cancel(
        &mut self,
        reason: String,
    ) -> Result<AotSseCallbackState, String> {
        self.invocation.cancel_pending(reason.clone())?;
        let state = self.invoke(
            AotSseCallbackEvent::Cancellation,
            vec![ReplValue::String(reason)],
        );
        self.live.close();
        self.invocation
            .finish_terminal(AotSseCallbackEvent::Cancellation, state?)
    }

    /// Resumes the exact parked callback from one typed VM I/O wake.
    pub(in crate::commands::serve) fn resume(
        &mut self,
        wake: PureNativeIoWake,
    ) -> Result<AotSseCallbackState, String> {
        self.invocation.resume(wake)
    }

    /// Starts one event using its statically selected callback.
    fn invoke(
        &mut self,
        event: AotSseCallbackEvent,
        args: Vec<ReplValue>,
    ) -> Result<AotSseCallbackState, String> {
        let callback = self.callback(event).cloned();
        self.invocation.invoke(event, callback.as_ref(), args)
    }

    /// Selects the static callback assigned to one lifecycle event.
    fn callback(&self, event: AotSseCallbackEvent) -> Option<&VmNativeCallableRef> {
        let callbacks = self.callbacks.as_ref()?;
        Some(match event {
            AotSseCallbackEvent::Open => &callbacks.open,
            AotSseCallbackEvent::EventReady => &callbacks.event_ready,
            AotSseCallbackEvent::KeepAlive => &callbacks.keep_alive,
            AotSseCallbackEvent::Drain => &callbacks.drain,
            AotSseCallbackEvent::Cancellation => &callbacks.cancellation,
        })
    }
}

#[cfg(test)]
#[path = "sse_invocation_test.rs"]
mod sse_invocation_test;
