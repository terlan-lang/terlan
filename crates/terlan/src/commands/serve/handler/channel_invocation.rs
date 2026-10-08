//! Shared native invocation ownership for VM HTTP channel callbacks.

use std::fmt::Debug;
use std::sync::Arc;

use crate::commands::serve::handler_cache::invocation::{
    AotHandlerInvocation, AotHandlerInvocationStep,
};
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::pure_native::PureNativeIoWait;
use crate::runtime::vm::pure_native::PureNativeIoWake;
use crate::runtime::vm::ReplValue;

/// Observable completion state after dispatching or resuming one callback.
pub(in crate::commands::serve) type AotChannelCallbackState =
    terlan_runtime_abi::CallbackState<ReplValue, PureNativeIoWait>;

#[cfg(test)]
#[path = "channel_closure_test.rs"]
mod closure_test;

mod package_adapter;

/// Channel-neutral linear owner for generated callback invocation state.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotChannelInvocation<Event> {
    channel: &'static str,
    runtime: Arc<AotHandlerRuntime>,
    router_module: String,
    pending: Option<AotHandlerInvocation>,
    pending_event: Option<Event>,
    #[cfg(test)]
    completed_events: Vec<Event>,
}

impl<Event> AotChannelInvocation<Event>
where
    Event: Copy + Debug,
{
    /// Creates an empty callback owner bound to one admitted image generation.
    pub(in crate::commands::serve) fn new(
        channel: &'static str,
        runtime: Arc<AotHandlerRuntime>,
        router_module: String,
    ) -> Self {
        Self {
            channel,
            runtime,
            router_module,
            pending: None,
            pending_event: None,
            #[cfg(test)]
            completed_events: Vec::new(),
        }
    }

    /// Returns callback events that completed without retained execution state.
    #[cfg(test)]
    pub(in crate::commands::serve) fn completed_events(&self) -> &[Event] {
        &self.completed_events
    }

    /// Returns whether generated callback state is currently parked.
    pub(in crate::commands::serve) fn is_waiting(&self) -> bool {
        self.pending.is_some()
    }

    /// Returns the exact typed wait retained by the parked callback, if any.
    pub(in crate::commands::serve) fn pending_wait(
        &self,
    ) -> Result<Option<PureNativeIoWait>, String> {
        self.pending
            .as_ref()
            .map(AotHandlerInvocation::wait)
            .transpose()
    }

    /// Starts one callback after enforcing linear per-channel execution.
    pub(in crate::commands::serve) fn invoke(
        &mut self,
        event: Event,
        callback: Option<&ReplValue>,
        args: Vec<ReplValue>,
    ) -> Result<AotChannelCallbackState, String> {
        self.require_idle(event)?;
        let Some(callback) = callback else {
            #[cfg(test)]
            self.completed_events.push(event);
            return Ok(AotChannelCallbackState::Complete(ReplValue::Unit));
        };
        let step = self
            .runtime
            .begin_callable_invocation(&self.router_module, callback, args)
            .map_err(|error| self.callback_error(event, error))?;
        self.finish_step(event, step)
    }

    /// Completes setup work through the existing protocol-owned worker pump.
    /// The mutable borrow prevents another callback from entering while parked.
    pub(in crate::commands::serve) async fn invoke_suspendable(
        &mut self,
        event: Event,
        callback: &ReplValue,
        args: Vec<ReplValue>,
    ) -> Result<ReplValue, String> {
        self.require_idle(event)?;
        let value = self
            .runtime
            .execute_suspendable_callable(&self.router_module, callback, args)
            .await
            .map_err(|error| self.callback_error(event, error))?;
        #[cfg(test)]
        self.completed_events.push(event);
        Ok(value)
    }

    fn callback_error(&self, event: Event, error: impl std::fmt::Display) -> String {
        format!(
            "error[serve.{}.callback]: {event:?} callback failed: {error}",
            self.channel
        )
    }

    fn require_idle(&self, event: Event) -> Result<(), String> {
        if self.pending.is_some() {
            return Err(format!(
                "error[serve.{}.callback_busy]: cannot dispatch {event:?} while {:?} is waiting",
                self.channel, self.pending_event
            ));
        }
        Ok(())
    }

    /// Resumes the exact parked callback from one typed VM I/O wake.
    pub(in crate::commands::serve) fn resume(
        &mut self,
        wake: PureNativeIoWake,
    ) -> Result<AotChannelCallbackState, String> {
        let invocation = self.pending.take().ok_or_else(|| {
            format!(
                "error[serve.{}.callback_state]: no callback is waiting",
                self.channel
            )
        })?;
        let event = self.pending_event.take().ok_or_else(|| {
            format!(
                "error[serve.{}.callback_state]: waiting callback has no event owner",
                self.channel
            )
        })?;
        self.finish_step(event, invocation.resume(wake)?)
    }

    /// Cancels and releases currently parked callback state, if present.
    pub(in crate::commands::serve) fn cancel_pending(
        &mut self,
        reason: String,
    ) -> Result<(), String> {
        if let Some(invocation) = self.pending.take() {
            self.pending_event = None;
            invocation.cancel(reason)?;
        }
        Ok(())
    }

    /// Converts one shared invocation step into channel-owned state.
    fn finish_step(
        &mut self,
        event: Event,
        mut step: AotHandlerInvocationStep,
    ) -> Result<AotChannelCallbackState, String> {
        loop {
            step = match step {
                AotHandlerInvocationStep::Complete(value) => {
                    #[cfg(test)]
                    self.completed_events.push(event);
                    return Ok(AotChannelCallbackState::Complete(value));
                }
                AotHandlerInvocationStep::Waiting(invocation) => {
                    let wait = invocation.wait()?;
                    self.pending = Some(invocation);
                    self.pending_event = Some(event);
                    return Ok(AotChannelCallbackState::Waiting(wait));
                }
                AotHandlerInvocationStep::CapabilityWaiting(invocation) => {
                    invocation.resume_from_trusted_host().map_err(|error| {
                        format!("error[serve.{}.callback_capability]: {error}", self.channel)
                    })?
                }
                AotHandlerInvocationStep::TimerWaiting(invocation) => {
                    let reason = format!(
                    "error[serve.{}.timer_orchestration]: channel callbacks cannot retain an HTTP protocol deadline",
                    self.channel
                );
                    invocation.cancel(reason.clone())?;
                    return Err(reason);
                }
            };
        }
    }
}
