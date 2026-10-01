//! Bounded SSE protocol state. The host owns scheduling, I/O, and resource lifetime.

use std::collections::VecDeque;

use crate::channel_plan::{SseEndpointPlan, SseError};

/// Protocol queue snapshot, independent of VM process or socket identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SseStreamInfo {
    pub pending_events: usize,
    pub max_pending_events: usize,
    pub max_event_bytes: usize,
    pub closed: bool,
    pub emitted_events: usize,
}

/// One source-selected endpoint and its finite queue of maintained-codec frames.
/// Callbacks are opaque values: this type never executes or schedules them.
#[derive(Debug)]
pub struct SseSession<C> {
    plan: SseEndpointPlan<C>,
    pending: VecDeque<Vec<u8>>,
    closed: bool,
    emitted_events: usize,
}

impl<C> SseSession<C> {
    /// Endpoint construction/deserialization has already validated positive bounds.
    pub fn open(plan: SseEndpointPlan<C>) -> Self {
        Self {
            plan,
            pending: VecDeque::new(),
            closed: false,
            emitted_events: 0,
        }
    }

    pub fn plan(&self) -> &SseEndpointPlan<C> {
        &self.plan
    }

    pub fn is_open(&self) -> bool {
        !self.closed
    }

    /// Encodes once before admission; rejection leaves the queue and counters intact.
    pub fn enqueue(
        &mut self,
        id: Option<&str>,
        event: Option<&str>,
        retry_ms: Option<i64>,
        data: &str,
    ) -> Result<(), SseError> {
        if self.closed {
            return Err(SseError::Closed);
        }
        if self.pending.len() >= self.plan.max_pending_events() {
            return Err(SseError::BackpressureExceeded);
        }
        let encoded = crate::encode_event(id, event, retry_ms, data)
            .map_err(|error| SseError::Codec(error.to_string()))?;
        if encoded.len() > self.plan.max_event_bytes() {
            return Err(SseError::EventTooLarge);
        }
        self.pending.push_back(encoded.into_bytes());
        Ok(())
    }

    /// Transfers one frame to the host writer; this does not acknowledge socket delivery.
    pub fn flush_next(&mut self) -> Option<Vec<u8>> {
        let frame = self.pending.pop_front()?;
        self.emitted_events = self.emitted_events.saturating_add(1);
        Some(frame)
    }

    /// Rejects new events while retaining admitted frames for graceful drain.
    pub fn close(&mut self) {
        self.closed = true;
    }

    pub fn inspect(&self) -> SseStreamInfo {
        SseStreamInfo {
            pending_events: self.pending.len(),
            max_pending_events: self.plan.max_pending_events(),
            max_event_bytes: self.plan.max_event_bytes(),
            closed: self.closed,
            emitted_events: self.emitted_events,
        }
    }
}

#[cfg(test)]
#[path = "sse_session_test.rs"]
mod tests;
