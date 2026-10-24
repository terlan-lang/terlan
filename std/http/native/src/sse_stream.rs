//! Pull-driven source production. Hyper demand and VM callback futures own progress.

use super::*;
use bytes::Bytes;
use futures_util::{stream, StreamExt};
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use terlan_runtime_abi::{DescriptorValue, DescriptorView};

impl<I> SseCallbacks<I>
where
    I: CallbackInvocation<SseEvent> + Send + 'static,
    I::Value: Clone + Send + Sync,
{
    /// Admits a pull stream without executing application code. Open runs on
    /// first body demand under the same executor and cancellation lease as production.
    pub fn prepare_pull(invocation: I, live: SseSession<I::Value>) -> Result<Self, ServiceError> {
        if !live.is_open() || !live.plan().supports_pull_transport() {
            return Err("error[serve.sse.producer]: deferred open requires a live pull endpoint without independent keep-alive scheduling".into());
        }
        Ok(Self {
            live,
            invocation,
            open_pending: true,
        })
    }

    /// Retains at most one producer invocation and one bounded encoded frame.
    /// Dropping the HTTP body drops its VM continuation and cancels the session.
    pub fn into_response_body(self) -> Result<crate::response_body::ResponseBody, ServiceError> {
        if !self.plan().supports_pull_transport() {
            return Err("error[serve.sse.producer]: pull transport requires a producer without independent keep-alive scheduling".into());
        }
        let frames = stream::try_unfold(self, |mut session| async move {
            match session.next_produced_frame().await {
                Ok(frame) => Ok(frame.map(|bytes| (Frame::data(Bytes::from(bytes)), session))),
                Err(error) => {
                    let message = error.to_string();
                    match SseCancellation::cancel(&mut session, message.clone()) {
                        Ok(()) => Err(error),
                        Err(cleanup) => {
                            Err(format!("{message}; cancellation failed: {cleanup}").into())
                        }
                    }
                }
            }
        });
        Ok(crate::response_body::ResponseBody::Live(
            StreamBody::new(frames.fuse()).boxed_unsync(),
        ))
    }

    async fn next_produced_frame(&mut self) -> Result<Option<Vec<u8>>, ServiceError> {
        self.open_producer().await?;
        if let Some(frame) = self.flush_next_event() {
            return Ok(Some(frame));
        }
        self.require_open()?;
        let producer = self.plan().producer().cloned().ok_or_else(|| {
            ServiceError::from("error[serve.sse.producer]: missing source producer")
        })?;
        let value = self
            .invocation
            .invoke_suspendable(SseEvent::Produce, &producer, vec![])
            .await?;
        let Some((encoded, data)) = produced_event(&value)? else {
            self.drain_producer().await?;
            return Ok(None);
        };
        if encoded.len() > self.plan().max_event_bytes() {
            return Err("error[serve.sse.queue]: EventTooLarge".into());
        }
        // The source producer already uses the maintained codec. No second
        // encoder, queue, task, or speculative producer call is needed here.
        if let Some(callback) = self.plan().callback(SseEvent::EventReady).cloned() {
            self.invocation
                .invoke_suspendable(SseEvent::EventReady, &callback, vec![data.into()])
                .await?;
        }
        self.live.record_frame_transfer();
        Ok(Some(encoded.into_bytes()))
    }

    async fn open_producer(&mut self) -> Result<(), ServiceError> {
        if self.open_pending {
            self.require_open()?;
            if let Some(callback) = self.plan().callback(SseEvent::Open).cloned() {
                self.invocation
                    .invoke_suspendable(SseEvent::Open, &callback, vec![])
                    .await?;
            }
            self.open_pending = false;
        }
        Ok(())
    }

    async fn drain_producer(&mut self) -> Result<(), ServiceError> {
        self.require_open()?;
        if let Err(error) = self
            .invocation
            .cancel_pending("SSE transport began graceful drain".into())
        {
            self.live.close();
            return Err(error.into());
        }
        // Keep the lease open across await so body cancellation still releases
        // the callback and dispatches cancellation. EOF follows successful drain.
        let result = match self.plan().callback(SseEvent::Drain).cloned() {
            Some(callback) => self
                .invocation
                .invoke_suspendable(SseEvent::Drain, &callback, vec![])
                .await
                .map(|_| ()),
            None => Ok(()),
        };
        self.live.close();
        result.map_err(ServiceError::from)
    }
}

fn produced_event<V: DescriptorValue>(value: &V) -> Result<Option<(String, String)>, ServiceError> {
    use crate::source_descriptor::{record, text};
    match value.descriptor_view() {
        DescriptorView::Atom("none") | DescriptorView::Record("None", []) => Ok(None),
        _ => {
            let [value] = record(value, "Some", ["value"]).map_err(descriptor_error)?;
            let DescriptorView::Tuple([encoded, data]) = value.descriptor_view() else {
                return Err(
                    "error[serve.sse.producer_result]: expected encoded event and data".into(),
                );
            };
            let encoded = text(encoded).map_err(descriptor_error)?;
            let data = text(data).map_err(descriptor_error)?;
            if encoded.is_empty() {
                return Err("error[serve.sse.producer_result]: encoded event is empty".into());
            }
            Ok(Some((encoded, data)))
        }
    }
}

fn descriptor_error(error: terlan_runtime_abi::NativeAdapterError) -> ServiceError {
    format!("error[{}]: {}", error.code(), error.message()).into()
}

#[cfg(test)]
#[path = "sse_stream_test.rs"]
mod tests;
