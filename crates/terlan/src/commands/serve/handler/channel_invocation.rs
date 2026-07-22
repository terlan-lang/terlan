#![cfg_attr(not(test), allow(dead_code))]

//! Shared native invocation ownership for VM HTTP channel callbacks.

use std::fmt::Debug;
use std::sync::Arc;

use crate::commands::serve::handler_cache::invocation::{
    AotHandlerInvocation, AotHandlerInvocationStep,
};
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::native_callable::VmNativeCallableRef;
use crate::runtime::vm::pure_native::{PureNativeIoWait, PureNativeIoWake};
use crate::runtime::vm::ReplValue;

/// Observable completion state after dispatching or resuming one channel callback.
#[derive(Debug)]
pub(in crate::commands::serve) enum AotChannelCallbackState {
    /// Callback returned without retaining generated execution state.
    Complete(ReplValue),
    /// Callback parked on one exact typed VM I/O wait.
    Waiting(PureNativeIoWait),
}

/// Channel-neutral linear owner for generated callback invocation state.
#[derive(Debug)]
pub(in crate::commands::serve) struct AotChannelInvocation<Event> {
    channel: &'static str,
    runtime: Arc<AotHandlerRuntime>,
    module: String,
    pending: Option<AotHandlerInvocation>,
    pending_event: Option<Event>,
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
        module: String,
    ) -> Self {
        Self {
            channel,
            runtime,
            module,
            pending: None,
            pending_event: None,
            completed_events: Vec::new(),
        }
    }

    /// Returns callback events that completed without retained execution state.
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
        callback: Option<&VmNativeCallableRef>,
        args: Vec<ReplValue>,
    ) -> Result<AotChannelCallbackState, String> {
        if self.pending.is_some() {
            return Err(format!(
                "error[serve.{}.callback_busy]: cannot dispatch {event:?} while {:?} is waiting",
                self.channel, self.pending_event
            ));
        }
        let Some(callback) = callback else {
            self.completed_events.push(event);
            return Ok(AotChannelCallbackState::Complete(ReplValue::Unit));
        };
        let step = self
            .runtime
            .begin_request_invocation(&self.module, &callback.function, args)
            .map_err(|error| {
                format!(
                    "error[serve.{}.callback]: {event:?} callback `{}.{}/{}` failed: {error}",
                    self.channel, callback.module, callback.function, callback.arity
                )
            })?;
        self.finish_step(event, step)
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

    /// Accepts a terminal callback only when it released generated state.
    pub(in crate::commands::serve) fn finish_terminal(
        &mut self,
        event: Event,
        state: AotChannelCallbackState,
    ) -> Result<AotChannelCallbackState, String> {
        if matches!(state, AotChannelCallbackState::Waiting(_)) {
            self.cancel_pending(format!("terminal {event:?} callback cannot suspend"))?;
            return Err(format!(
                "error[serve.{}.terminal_wait]: terminal {event:?} callback cannot suspend",
                self.channel
            ));
        }
        Ok(state)
    }

    /// Converts one shared invocation step into channel-owned state.
    fn finish_step(
        &mut self,
        event: Event,
        step: AotHandlerInvocationStep,
    ) -> Result<AotChannelCallbackState, String> {
        match step {
            AotHandlerInvocationStep::Complete(value) => {
                self.completed_events.push(event);
                Ok(AotChannelCallbackState::Complete(value))
            }
            AotHandlerInvocationStep::Waiting(invocation) => {
                let wait = invocation.wait()?;
                self.pending = Some(invocation);
                self.pending_event = Some(event);
                Ok(AotChannelCallbackState::Waiting(wait))
            }
        }
    }
}
