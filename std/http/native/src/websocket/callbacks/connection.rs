//! Package transport adaptation for opaque host callback execution.

use super::WebSocketCallbacks;
use crate::channel_plan::{WebSocketEndpointPlan, WebSocketEvent};
use crate::source_descriptor::PairedTransition;
use crate::websocket::hub::AdmissionCallbacks;
use crate::websocket::{connection::Callbacks, Utf8Bytes};
use terlan_runtime_abi::{CallbackInvocation, DescriptorValue, OwnedDescriptor};

impl<I: CallbackInvocation<WebSocketEvent>> AdmissionCallbacks for WebSocketCallbacks<I> {
    fn room_identity(&mut self, sequence: i64) -> Result<String, crate::ServiceError> {
        self.dispatch_pair_room_identity_output(sequence)
    }
    fn matched(
        &mut self,
        room: String,
        first: String,
        second: String,
    ) -> Result<(String, String), crate::ServiceError> {
        self.dispatch_pair_matched_output(room, first, second)
    }

    fn restored(
        &mut self,
        room: String,
        state: String,
        role: i64,
        first: String,
        second: String,
    ) -> Result<String, crate::ServiceError> {
        self.dispatch_pair_restored_output(room, state, role, first, second)
    }
}

impl<I: CallbackInvocation<WebSocketEvent>> Callbacks for WebSocketCallbacks<I>
where
    I::Value: Clone,
{
    type Callback = I::Value;

    fn plan(&self) -> &WebSocketEndpointPlan<I::Value> {
        self.plan()
    }

    async fn identity(
        &mut self,
        target: String,
    ) -> Result<Option<(String, i64)>, crate::ServiceError> {
        self.dispatch_pair_identity_output(target).await
    }

    fn waiting(&mut self) -> Result<String, crate::ServiceError> {
        self.dispatch_pair_waiting_output()
    }
    fn peer_left(&mut self) -> Result<String, crate::ServiceError> {
        self.dispatch_pair_peer_left_output()
    }
    fn enqueue(&mut self, text: Utf8Bytes) -> Result<(), crate::ServiceError> {
        self.enqueue_inbound(text)
    }

    fn next_inbound(&mut self) -> Result<(bool, Option<String>), crate::ServiceError> {
        let (dispatched, output) = self.dispatch_next_inbound_output()?;
        let payload = match output.map(DescriptorValue::into_descriptor) {
            Some(OwnedDescriptor::String(payload)) => Some(payload),
            Some(OwnedDescriptor::Unit) | None => None,
            Some(_) => return Err(
                "error[serve.websocket.callback_result]: inbound callback expected String or Unit"
                    .into(),
            ),
        };
        Ok((dispatched, payload))
    }

    fn next_paired(
        &mut self,
        context: Option<crate::source_descriptor::PairContext>,
    ) -> Result<Option<PairedTransition>, crate::ServiceError> {
        let (dispatched, output) = self.dispatch_next_paired_inbound_output(context)?;
        if !dispatched {
            return Ok(None);
        }
        let output = output.ok_or_else(|| "error[serve.websocket.callback_result]: stateful paired callback suspended without producing a transition".to_string())?;
        crate::source_descriptor::paired_callback_transition(output)
            .map(Some)
            .map_err(|error| {
                format!(
                    "error[serve.websocket.callback_result]: {}",
                    error.message()
                )
                .into()
            })
    }

    fn writable(&mut self) -> Result<(), crate::ServiceError> {
        if !self.is_waiting() {
            self.writable()?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), crate::ServiceError> {
        self.close().map(|_| ())
    }
    fn cancel(&mut self, reason: String) -> Result<(), crate::ServiceError> {
        self.cancel(reason).map(|_| ())
    }
}
