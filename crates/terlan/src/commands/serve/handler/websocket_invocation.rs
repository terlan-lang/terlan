//! VM construction adapter; callback protocol policy belongs to std.http.

use std::sync::Arc;
#[cfg(test)]
use terlan_http_native::channel_plan::WebSocketEndpointPlan;
use terlan_http_native::channel_plan::WebSocketEvent as AotWebSocketCallbackEvent;
use terlan_http_native::websocket::callbacks::WebSocketCallbacks;
use terlan_http_native::websocket::session::Session;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::ReplValue;

#[cfg(test)]
use super::channel_invocation::AotChannelCallbackState;
use super::channel_invocation::AotChannelInvocation;

#[cfg(test)]
type AotWebSocketCallbackState = AotChannelCallbackState;

pub(in crate::commands::serve) type AotWebSocketCallbackSession =
    WebSocketCallbacks<AotChannelInvocation<AotWebSocketCallbackEvent>>;

pub(in crate::commands::serve) fn open(
    runtime: Arc<AotHandlerRuntime>,
    module: String,
    live: Session<ReplValue>,
) -> Result<AotWebSocketCallbackSession, String> {
    AotWebSocketCallbackSession::open(
        AotChannelInvocation::new("websocket", runtime, module),
        live,
    )
    .map_err(String::from)
}

#[cfg(test)]
#[path = "websocket_invocation_test.rs"]
#[cfg(test)]
mod websocket_invocation_test;

#[cfg(test)]
#[path = "websocket_identity_test.rs"]
mod websocket_identity_test;
