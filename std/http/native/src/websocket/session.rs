//! Source-selected text-channel state; transport and callback execution stay host-owned.

use std::collections::VecDeque;

use super::Utf8Bytes;
use crate::channel_plan::WebSocketEndpointPlan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboundQueueInfo {
    pub pending_frames: usize,
    pub max_pending_frames: usize,
    pub queued_frame_bytes: usize,
    pub max_frame_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    Closed,
    FrameTooLarge,
    Full,
    ByteCountOverflow,
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Closed => "session is closed",
            Self::FrameTooLarge => "frame exceeds max_frame_bytes",
            Self::Full => "pending frame queue is full",
            Self::ByteCountOverflow => "queued frame byte count overflow",
        };
        write!(formatter, "error[http.websocket.queue]: {message}")
    }
}

impl std::error::Error for QueueError {}

/// Retains admitted endpoint policy without interpreting or invoking callbacks.
/// Only maintained-codec text buffers enter this queue; control traffic stays
/// in the codec and never becomes a source application message.
#[derive(Debug)]
pub struct Session<C> {
    plan: WebSocketEndpointPlan<C>,
    inbound: VecDeque<Utf8Bytes>,
    queued_frame_bytes: usize,
    open: bool,
}

impl<C> Session<C> {
    pub fn open(plan: WebSocketEndpointPlan<C>) -> Self {
        Self {
            plan,
            inbound: VecDeque::new(),
            queued_frame_bytes: 0,
            open: true,
        }
    }

    pub fn plan(&self) -> &WebSocketEndpointPlan<C> {
        &self.plan
    }

    pub fn inspect(&self) -> InboundQueueInfo {
        InboundQueueInfo {
            pending_frames: self.inbound.len(),
            max_pending_frames: self.plan.max_pending_frames(),
            queued_frame_bytes: self.queued_frame_bytes,
            max_frame_bytes: self.plan.max_frame_bytes(),
        }
    }

    /// Moves an already validated UTF-8 buffer without copying its payload.
    pub fn enqueue_inbound(&mut self, text: Utf8Bytes) -> Result<(), QueueError> {
        if !self.open {
            return Err(QueueError::Closed);
        }
        if text.len() > self.plan.max_frame_bytes() {
            return Err(QueueError::FrameTooLarge);
        }
        if self.inbound.len() >= self.plan.max_pending_frames() {
            return Err(QueueError::Full);
        }
        let bytes = self
            .queued_frame_bytes
            .checked_add(text.len())
            .ok_or(QueueError::ByteCountOverflow)?;
        self.inbound.push_back(text);
        self.queued_frame_bytes = bytes;
        Ok(())
    }

    pub fn next_inbound(&mut self) -> Option<Utf8Bytes> {
        let text = self.inbound.pop_front()?;
        self.queued_frame_bytes -= text.len();
        Some(text)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Stops admission while retaining already admitted messages for host cleanup.
    pub fn close(&mut self) {
        self.open = false;
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod tests;
