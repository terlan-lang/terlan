//! VM construction adapter; callback protocol policy belongs to std.http.

use std::sync::Arc;
#[cfg(test)]
use terlan_http_native::channel_plan::SseEndpointPlan;
use terlan_http_native::channel_plan::SseEvent as AotSseCallbackEvent;
use terlan_http_native::sse_callbacks::SseCallbacks;
use terlan_http_native::sse_session::SseSession;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::ReplValue;

#[cfg(test)]
use super::channel_invocation::AotChannelCallbackState;
use super::channel_invocation::AotChannelInvocation;

#[cfg(test)]
type AotSseCallbackState = AotChannelCallbackState;

pub(in crate::commands::serve) type AotSseCallbackSession =
    SseCallbacks<AotChannelInvocation<AotSseCallbackEvent>>;

pub(in crate::commands::serve) fn open(
    runtime: Arc<AotHandlerRuntime>,
    module: String,
    live: SseSession<ReplValue>,
) -> Result<AotSseCallbackSession, String> {
    AotSseCallbackSession::open(AotChannelInvocation::new("sse", runtime, module), live)
        .map_err(String::from)
}

#[cfg(test)]
#[path = "sse_invocation_test.rs"]
mod sse_invocation_test;
