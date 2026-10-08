//! HTTP terminal callbacks must release their continuation before transport exit.

use std::fmt::Debug;

use crate::ServiceError;
use terlan_runtime_abi::{CallbackInvocation, CallbackState};

pub(crate) fn finish_terminal<E: Debug, I: CallbackInvocation<E>>(
    invocation: &mut I,
    event: E,
    state: CallbackState<I::Value, I::Wait>,
    channel: &str,
) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
    if matches!(state, CallbackState::Waiting(_)) {
        let reason = format!("terminal {event:?} callback cannot suspend");
        let error = format!("error[serve.{channel}.terminal_wait]: {reason}");
        return Err(match invocation.cancel_pending(reason) {
            Ok(()) => error,
            Err(cleanup) => format!("{error}; cancellation failed: {cleanup}"),
        }
        .into());
    }
    Ok(state)
}

#[cfg(test)]
#[path = "channel_completion_test.rs"]
mod tests;
