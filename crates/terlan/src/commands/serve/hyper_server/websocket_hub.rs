//! Executes source callbacks for the package-owned room registry.

pub(super) use terlan_http_native::websocket::hub::WebSocketHub;
use terlan_http_native::websocket::hub::{AdmissionCallbacks, WebSocketHubLease};

use super::super::handler::AotWebSocketCallbackSession;

impl AdmissionCallbacks for AotWebSocketCallbackSession {
    fn matched(
        &mut self,
        room: String,
        role: i64,
        first: String,
        second: String,
    ) -> Result<String, String> {
        self.dispatch_pair_matched_output(room, role, first, second)
    }

    fn restored(
        &mut self,
        room: String,
        state: String,
        role: i64,
        first: String,
        second: String,
    ) -> Result<String, String> {
        self.dispatch_pair_restored_output(room, state, role, first, second)
    }
}

pub(super) fn drain_stateful_inbound(
    lease: &WebSocketHubLease,
    session: &mut AotWebSocketCallbackSession,
) -> Result<(), String> {
    loop {
        let mut dispatched = false;
        lease.transition(|state, role, first_request, second_request| {
            let (did_dispatch, output) = session.dispatch_next_stateful_inbound_output(
                state.clone(), role, first_request, second_request,
            )?;
            dispatched = did_dispatch;
            if !did_dispatch {
                return Ok((state, None, None));
            }
            let output = output.ok_or_else(|| "error[serve.websocket.callback_result]: stateful paired callback suspended without producing a transition".to_string())?;
            terlan_http_native::source_descriptor::paired_transition(output)
                .map_err(|error| format!("error[serve.websocket.callback_result]: {}", error.message()))
        })?;
        if !dispatched {
            return Ok(());
        }
    }
}
