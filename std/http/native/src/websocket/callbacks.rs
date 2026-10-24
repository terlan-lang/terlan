//! Package-owned WebSocket callback lifecycle over opaque host execution.

use crate::ServiceError;

use terlan_runtime_abi::{
    CallbackInvocation, CallbackState, DescriptorValue, NativeValue, OwnedDescriptor,
};

use crate::channel_plan::{WebSocketEndpointPlan, WebSocketEvent};
use crate::websocket::session::{InboundQueueInfo, Session};
use crate::websocket::Utf8Bytes;

mod connection;
mod opening;
mod terminal;

#[cfg(test)]
#[path = "callbacks_test.rs"]
mod tests;

/// One WebSocket connection bound to a native image generation and callback set.
#[derive(Debug)]
pub struct WebSocketCallbacks<I: CallbackInvocation<WebSocketEvent>> {
    live: Session<I::Value>,
    invocation: I,
}

struct QueuedFrame<W> {
    text: String,
    wait: Option<W>,
}

impl<I: CallbackInvocation<WebSocketEvent>> WebSocketCallbacks<I> {
    /// Admits one live connection and immediately dispatches its open callback.
    pub fn open(invocation: I, live: Session<I::Value>) -> Result<Self, ServiceError> {
        let mut session = Self { live, invocation };
        if !session.is_open() {
            return Err("error[serve.websocket.closed]: connection is closed".into());
        }
        if let Err(error) = session.invoke(WebSocketEvent::Open, Vec::new()) {
            return Err(session.failed_open(error));
        }
        Ok(session)
    }

    fn failed_open(&mut self, error: ServiceError) -> ServiceError {
        match self.cancel(error.to_string()) {
            Ok(_) => error,
            Err(cleanup) => format!("{error}; cancellation failed: {cleanup}").into(),
        }
    }

    async fn notify_suspendable(&mut self, event: WebSocketEvent) -> Result<(), ServiceError> {
        if let Some(callback) = self.live.plan().callback(event) {
            self.invocation
                .invoke_suspendable(event, callback, Vec::new())
                .await
                .map(|_| ())
                .map_err(ServiceError::from)
        } else {
            // An absent callback has no source work that could own a wait.
            let state = self.invoke(event, Vec::new())?;
            crate::channel_completion::finish_terminal(
                &mut self.invocation,
                event,
                state,
                "websocket",
            )
            .map(|_| ())
        }
    }

    /// Returns whether the package-owned connection remains open.
    pub fn is_open(&self) -> bool {
        self.live.is_open()
    }

    /// Returns the immutable endpoint policy retained by the live connection.
    pub fn plan(&self) -> &WebSocketEndpointPlan<I::Value> {
        self.live.plan()
    }

    /// Returns bounded inbound queue state for transport admission checks.
    pub fn inspect(&self) -> InboundQueueInfo {
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

    /// Queues one decoded frame under the admitted endpoint pressure limits.
    pub fn enqueue_inbound(&mut self, text: Utf8Bytes) -> Result<(), ServiceError> {
        self.inbound_wait()?;
        self.live
            .enqueue_inbound(text)
            .map_err(|error| ServiceError::from(error.to_string()))
    }

    /// Dispatches or wakes generated code with the oldest queued text frame.
    pub fn dispatch_next_inbound(&mut self) -> Result<bool, ServiceError> {
        self.dispatch_next_inbound_output()
            .map(|(dispatched, _)| dispatched)
    }

    /// Dispatches one queued text frame and retains its source callback result.
    pub fn dispatch_next_inbound_output(
        &mut self,
    ) -> Result<(bool, Option<I::Value>), ServiceError> {
        let Some(QueuedFrame { text, wait }) = self.next_inbound_frame()? else {
            return Ok((false, None));
        };
        if let Some(wait) = wait {
            let state = self.resume(I::text_wake(wait, text))?;
            Ok((true, completed_value(state)))
        } else {
            let state = self.inbound(text)?;
            Ok((true, completed_value(state)))
        }
    }

    /// Awaits one paired source callback with a serialized room snapshot.
    pub async fn dispatch_next_paired_inbound_output(
        &mut self,
        context: Option<crate::source_descriptor::PairContext>,
    ) -> Result<(bool, Option<I::Value>), ServiceError> {
        let Some(QueuedFrame { text, wait }) = self.next_inbound_frame()? else {
            return Ok((false, None));
        };
        if let Some(wait) = wait {
            let state = self
                .invocation
                .resume_streaming(I::text_wake(wait, text))
                .await?;
            Ok((true, completed_value(state)))
        } else {
            let value = self
                .invoke_callback_suspendable(
                    WebSocketEvent::Inbound,
                    vec![
                        context
                            .map(|(state, role, first, second)| {
                                NativeValue::Tuple(vec![
                                    state.into(),
                                    role.into(),
                                    first.into(),
                                    second.into(),
                                ])
                            })
                            .into(),
                        NativeValue::String(text),
                    ],
                    "paired inbound",
                )
                .await?;
            Ok((true, Some(value)))
        }
    }

    fn inbound_wait(&self) -> Result<Option<I::Wait>, ServiceError> {
        let wait = self.invocation.pending_wait()?;
        if let Some(wait) = &wait {
            I::validate_text_wait(wait).map_err(|error| {
                format!("error[serve.websocket.wake_type]: inbound text {error}")
            })?;
        }
        Ok(wait)
    }

    /// Runs one frame through host timers/workers, retaining text waits for
    /// subsequent frames instead of blocking transport ownership.
    pub async fn dispatch_next_inbound_streaming(
        &mut self,
    ) -> Result<(bool, Option<I::Value>), ServiceError> {
        let Some(QueuedFrame { text, wait }) = self.next_inbound_frame()? else {
            return Ok((false, None));
        };
        let state = if let Some(wait) = wait {
            self.invocation
                .resume_streaming(I::text_wake(wait, text))
                .await?
        } else {
            self.invocation
                .invoke_streaming(
                    WebSocketEvent::Inbound,
                    self.live.plan().callback(WebSocketEvent::Inbound),
                    vec![NativeValue::String(text)],
                )
                .await?
        };
        Ok((true, completed_value(state)))
    }

    fn next_inbound_frame(&mut self) -> Result<Option<QueuedFrame<I::Wait>>, ServiceError> {
        if self.live.inspect().pending_frames == 0 {
            return Ok(None);
        }
        let wait = self.inbound_wait()?;
        Ok(self.live.next_inbound().map(|text| QueuedFrame {
            text: text.to_string(),
            wait,
        }))
    }

    /// Resolves reconnect identity using the admitted source callback.
    pub async fn dispatch_pair_identity_output(
        &mut self,
        target: String,
    ) -> Result<Option<(String, i64)>, ServiceError> {
        let callback = self
            .live
            .plan()
            .callback(WebSocketEvent::PairIdentity)
            .ok_or_else(|| {
                "error[serve.websocket.callback_result]: no reconnect identity callback".to_string()
            })?;
        let value = self
            .invocation
            .invoke_suspendable(
                WebSocketEvent::PairIdentity,
                callback,
                vec![NativeValue::String(target)],
            )
            .await?;
        crate::source_descriptor::restoration_identity(&value).map_err(|error| {
            ServiceError::from(format!("error[{}]: {}", error.code(), error.message()))
        })
    }

    pub async fn dispatch_pair_room_identity_output(
        &mut self,
        sequence: i64,
    ) -> Result<String, ServiceError> {
        self.invoke_string_callback_suspendable(
            WebSocketEvent::PairRoomIdentity,
            vec![NativeValue::Int(sequence)],
            "room identity",
        )
        .await
    }

    /// Builds both source-selected payloads after a fresh pair is formed.
    pub async fn dispatch_pair_matched_output(
        &mut self,
        room_id: String,
        first_request: String,
        second_request: String,
    ) -> Result<(String, String), ServiceError> {
        let value = self
            .invoke_callback_suspendable(
                WebSocketEvent::PairMatched,
                vec![
                    NativeValue::String(room_id),
                    NativeValue::String(first_request),
                    NativeValue::String(second_request),
                ],
                "paired matched",
            )
            .await?;
        crate::source_descriptor::paired_match(value).map_err(|error| {
            format!(
                "error[serve.websocket.callback_result]: {}",
                error.message()
            )
            .into()
        })
    }

    /// Builds the retained role-specific view for one reclaimed seat.
    pub async fn dispatch_pair_restored_output(
        &mut self,
        room_id: String,
        state: String,
        role: i64,
        first_request: String,
        second_request: String,
    ) -> Result<String, ServiceError> {
        self.invoke_string_callback_suspendable(
            WebSocketEvent::PairRestored,
            vec![
                NativeValue::String(room_id),
                NativeValue::String(state),
                NativeValue::Int(role),
                NativeValue::String(first_request),
                NativeValue::String(second_request),
            ],
            "paired restored",
        )
        .await
    }

    /// Builds the typed payload sent while the first peer waits for a match.
    pub async fn dispatch_pair_waiting_output(&mut self) -> Result<String, ServiceError> {
        self.invoke_string_callback_suspendable(
            WebSocketEvent::PairWaiting,
            Vec::new(),
            "paired waiting payload",
        )
        .await
    }

    /// Builds the typed payload sent when a paired peer disconnects.
    pub async fn dispatch_pair_peer_left_output(&mut self) -> Result<String, ServiceError> {
        self.invoke_string_callback_suspendable(
            WebSocketEvent::PairPeerLeft,
            Vec::new(),
            "paired peer-left payload",
        )
        .await
    }

    /// Dispatches one admitted inbound text frame through generated code.
    pub fn inbound(
        &mut self,
        value: String,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invoke(WebSocketEvent::Inbound, vec![NativeValue::String(value)])
    }

    /// Dispatches one writable transport notification through generated code.
    pub fn writable(&mut self) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invoke(WebSocketEvent::Writable, Vec::new())
    }

    /// Cancels parked work, dispatches cancellation, and ends the live lease.
    pub fn cancel(
        &mut self,
        reason: String,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.live.close();
        self.invocation.cancel_pending(reason.clone())?;
        let state = self.invoke(
            WebSocketEvent::Cancellation,
            vec![NativeValue::String(reason)],
        );
        crate::channel_completion::finish_terminal(
            &mut self.invocation,
            WebSocketEvent::Cancellation,
            state?,
            "websocket",
        )
    }

    /// Resumes the exact parked callback from one typed VM I/O wake.
    pub fn resume(
        &mut self,
        wake: I::Wake,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invocation.resume(wake).map_err(ServiceError::from)
    }

    /// Starts one event using its statically selected callback.
    fn invoke(
        &mut self,
        event: WebSocketEvent,
        args: Vec<NativeValue>,
    ) -> Result<CallbackState<I::Value, I::Wait>, ServiceError> {
        self.invocation
            .invoke(event, self.live.plan().callback(event), args)
            .map_err(ServiceError::from)
    }

    async fn invoke_callback_suspendable(
        &mut self,
        event: WebSocketEvent,
        args: Vec<NativeValue>,
        label: &str,
    ) -> Result<I::Value, ServiceError> {
        let callback = self.live.plan().callback(event).ok_or_else(|| {
            format!("error[serve.websocket.callback_result]: no {label} callback")
        })?;
        self.invocation
            .invoke_suspendable(event, callback, args)
            .await
            .map_err(ServiceError::from)
    }

    async fn invoke_string_callback_suspendable(
        &mut self,
        event: WebSocketEvent,
        args: Vec<NativeValue>,
        label: &str,
    ) -> Result<String, ServiceError> {
        let value = self.invoke_callback_suspendable(event, args, label).await?;
        string_payload(value, label)
    }
}

fn string_payload(value: impl DescriptorValue, label: &str) -> Result<String, ServiceError> {
    match value.into_descriptor() {
        OwnedDescriptor::String(payload) => Ok(payload),
        _ => Err(format!(
            "error[serve.websocket.callback_result]: {label} callback returned a non-String value, expected String"
        ).into()),
    }
}

fn completed_value<V, W>(state: CallbackState<V, W>) -> Option<V> {
    match state {
        CallbackState::Complete(value) => Some(value),
        CallbackState::Waiting(_) => None,
    }
}
