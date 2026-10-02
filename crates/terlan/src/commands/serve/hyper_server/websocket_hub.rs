//! Executes source callbacks for the package-owned room registry.

use crate::runtime::vm::ReplValue;
use terlan_http_native::channel_plan::WebSocketEndpointPlan;
use terlan_http_native::source_descriptor::PairedTransition;
use terlan_http_native::websocket::hub::AdmissionCallbacks;
pub(super) use terlan_http_native::websocket::hub::WebSocketHub;
use terlan_http_native::websocket::{connection::Callbacks, Utf8Bytes};

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

impl Callbacks for AotWebSocketCallbackSession {
    type Callback = ReplValue;

    fn plan(&self) -> &WebSocketEndpointPlan<ReplValue> {
        self.plan()
    }

    async fn identity(&mut self, target: String) -> Result<Option<(String, i64)>, String> {
        self.dispatch_pair_identity_output(target).await
    }

    fn waiting(&mut self) -> Result<String, String> {
        self.dispatch_pair_waiting_output()
    }
    fn peer_left(&mut self) -> Result<String, String> {
        self.dispatch_pair_peer_left_output()
    }
    fn enqueue(&mut self, text: Utf8Bytes) -> Result<(), String> {
        self.enqueue_inbound(text)
    }

    fn next_inbound(&mut self) -> Result<(bool, Option<String>), String> {
        let (dispatched, output) = self.dispatch_next_inbound_output()?;
        let payload = match output {
            Some(ReplValue::String(payload)) => Some(payload),
            Some(ReplValue::Unit) | None => None,
            Some(value) => return Err(format!("error[serve.websocket.callback_result]: inbound callback returned {value:?}, expected String or Unit")),
        };
        Ok((dispatched, payload))
    }

    fn next_stateful(
        &mut self,
        state: String,
        role: i64,
        first: String,
        second: String,
    ) -> Result<Option<PairedTransition>, String> {
        let (dispatched, output) =
            self.dispatch_next_stateful_inbound_output(state, role, first, second)?;
        if !dispatched {
            return Ok(None);
        }
        let output = output.ok_or_else(|| "error[serve.websocket.callback_result]: stateful paired callback suspended without producing a transition".to_string())?;
        terlan_http_native::source_descriptor::paired_transition(output)
            .map(Some)
            .map_err(|error| {
                format!(
                    "error[serve.websocket.callback_result]: {}",
                    error.message()
                )
            })
    }

    fn writable(&mut self) -> Result<(), String> {
        if !self.is_waiting() {
            self.writable()?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), String> {
        self.close().map(|_| ())
    }
    fn cancel(&mut self, reason: String) -> Result<(), String> {
        self.cancel(reason).map(|_| ())
    }
}
