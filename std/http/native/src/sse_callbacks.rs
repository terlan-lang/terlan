//! Package-owned SSE callback lifecycle over opaque host execution.

use crate::ServiceError;

use terlan_runtime_abi::{CallbackInvocation, CallbackState, NativeValue};

use crate::channel_plan::{SseEndpointPlan, SseEvent};
use crate::sse_session::{SseSession, SseStreamInfo};

#[cfg(test)]
#[path = "sse_callbacks_test.rs"]
mod tests;

/// Transport rejection must terminate an admitted source session explicitly.
pub trait SseCancellation {
    fn cancel(&mut self, reason: String) -> Result<(), ServiceError>;
}

impl<I: CallbackInvocation<SseEvent>> SseCancellation for SseCallbacks<I> {
    fn cancel(&mut self, reason: String) -> Result<(), ServiceError> {
        SseCallbacks::cancel(self, reason).map(|_| ())
    }
}

/// One SSE stream bound to a native image generation and callback set.
#[derive(Debug)]
pub struct SseCallbacks<I: CallbackInvocation<SseEvent>> {
    live: SseSession<I::Value>,
    invocation: I,
}

impl<I: CallbackInvocation<SseEvent>> SseCallbacks<I> {
    /// Admits one live stream and immediately dispatches its open callback.
    pub fn open(invocation: I, live: SseSession<I::Value>) -> Result<Self, ServiceError> {
        let mut session = Self { live, invocation };
        session.invoke(SseEvent::Open, Vec::new())?;
        Ok(session)
    }

    /// Returns whether the package-owned stream remains open.
    pub fn is_open(&self) -> bool {
        self.live.is_open()
    }

    /// Returns the immutable endpoint policy retained by the live stream.
    pub fn plan(&self) -> &SseEndpointPlan<I::Value> {
        self.live.plan()
    }

    /// Returns bounded queue state for transport admission checks.
    pub fn inspect(&self) -> SseStreamInfo {
        self.live.inspect()
    }

    /// Host instrumentation can inspect execution without mutating protocol state.
    pub fn executor(&self) -> &I {
        &self.invocation
    }

    /// Returns whether generated callback work is parked on typed VM I/O.
    pub fn is_waiting(&self) -> bool {
        self.invocation.is_waiting()
    }

    /// Queues one data event and dispatches or wakes its generated callback.
    pub fn enqueue_event(
        &mut self,
        data: String,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        let pending = self.invocation.pending_wait()?;
        if let Some(wait) = &pending {
            I::validate_text_wait(wait)
                .map_err(|error| format!("error[serve.sse.wake_type]: event data {error}"))?;
        }
        self.live
            .enqueue(None, None, None, &data)
            .map_err(|error| format!("error[serve.sse.queue]: {error:?}"))?;
        if let Some(wait) = pending {
            self.resume(I::text_wake(wait, data))
        } else {
            self.event_ready(data)
        }
    }

    /// Transfers the oldest encoded event to HTTP stream transport.
    pub fn flush_next_event(&mut self) -> Option<Vec<u8>> {
        self.live.flush_next()
    }

    /// Dispatches one ready application event through generated code.
    pub fn event_ready(
        &mut self,
        data: String,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invoke(SseEvent::EventReady, vec![NativeValue::String(data)])
    }

    /// Dispatches one VM keep-alive notification through generated code.
    pub fn keep_alive(&mut self) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invoke(SseEvent::KeepAlive, Vec::new())
    }

    /// Dispatches graceful drain and ends the live-session lease.
    pub fn drain(&mut self) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.live.close();
        self.invocation
            .cancel_pending("SSE transport began graceful drain".to_string())?;
        let state = self.invoke(SseEvent::Drain, Vec::new());
        crate::channel_completion::finish_terminal(
            &mut self.invocation,
            SseEvent::Drain,
            state?,
            "sse",
        )
    }

    /// Cancels parked work, dispatches cancellation, and ends the live lease.
    pub fn cancel(
        &mut self,
        reason: String,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.live.close();
        self.invocation.cancel_pending(reason.clone())?;
        let state = self.invoke(SseEvent::Cancellation, vec![NativeValue::String(reason)]);
        crate::channel_completion::finish_terminal(
            &mut self.invocation,
            SseEvent::Cancellation,
            state?,
            "sse",
        )
    }

    /// Resumes the exact parked callback from one typed VM I/O wake.
    pub fn resume(
        &mut self,
        wake: I::Wake,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invocation.resume(wake).map_err(ServiceError::from)
    }

    /// Starts one event using its retained source callback.
    fn invoke(
        &mut self,
        event: SseEvent,
        args: Vec<NativeValue>,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invocation
            .invoke(event, self.live.plan().callback(event), args)
            .map_err(ServiceError::from)
    }
}
