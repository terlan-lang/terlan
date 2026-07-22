use std::collections::VecDeque;

use super::VmWebSocketFrame;
use crate::runtime::vm::{
    memory::{
        VmMemoryAccountant, VmMemoryPressureOutcome, VmSharedAllocationId, VmSharedAllocationKind,
    },
    process::{VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
};

/// Inspectable VM WebSocket inbound queue state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmWebSocketInboundQueueInfo {
    pub(crate) pending_frames: usize,
    pub(crate) max_pending_frames: usize,
    pub(crate) queued_frame_bytes: usize,
    pub(crate) max_frame_bytes: usize,
}

/// VM-owned bounded WebSocket inbound frame queue.
#[derive(Debug)]
pub(crate) struct VmWebSocketInboundQueue {
    max_pending_frames: usize,
    max_frame_bytes: usize,
    queued_frame_bytes: usize,
    frames: VecDeque<VmWebSocketFrame>,
}

impl VmWebSocketInboundQueue {
    pub(crate) fn new(max_pending_frames: usize, max_frame_bytes: usize) -> Self {
        Self {
            max_pending_frames,
            max_frame_bytes,
            queued_frame_bytes: 0,
            frames: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, frame: VmWebSocketFrame) -> Result<(), String> {
        let frame_bytes = self.validate_push(&frame)?;
        self.queued_frame_bytes = self.queued_frame_bytes.saturating_add(frame_bytes);
        self.frames.push_back(frame);
        Ok(())
    }

    fn validate_push(&self, frame: &VmWebSocketFrame) -> Result<usize, String> {
        let frame_bytes = frame.payload_len();
        if frame_bytes > self.max_frame_bytes {
            return Err("error[vm_websocket_queue]: frame exceeds max_frame_bytes".to_string());
        }
        if self.frames.len() >= self.max_pending_frames {
            return Err("error[vm_websocket_queue]: pending frame queue is full".to_string());
        }
        Ok(frame_bytes)
    }

    pub(crate) fn pop(&mut self) -> Option<VmWebSocketFrame> {
        let frame = self.frames.pop_front()?;
        self.queued_frame_bytes = self.queued_frame_bytes.saturating_sub(frame.payload_len());
        Some(frame)
    }

    pub(crate) fn inspect(&self) -> VmWebSocketInboundQueueInfo {
        VmWebSocketInboundQueueInfo {
            pending_frames: self.frames.len(),
            max_pending_frames: self.max_pending_frames,
            queued_frame_bytes: self.queued_frame_bytes,
            max_frame_bytes: self.max_frame_bytes,
        }
    }
}

/// Typed failure from a WebSocket queue governed by VM memory ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmAccountedWebSocketQueueError {
    Queue(String),
    Memory(String),
    MemoryPressureRejected,
}

/// WebSocket inbound queue whose frame buffers belong to one VM process.
#[derive(Debug)]
pub(crate) struct VmAccountedWebSocketInboundQueue {
    queue: VmWebSocketInboundQueue,
    owner: VmProcessId,
    allocations: VecDeque<VmSharedAllocationId>,
    cancelled: bool,
}

impl VmAccountedWebSocketInboundQueue {
    /// Opens a bounded inbound queue owned by one live VM process.
    pub(crate) fn new(
        owner: VmProcessId,
        max_pending_frames: usize,
        max_frame_bytes: usize,
    ) -> Self {
        Self {
            queue: VmWebSocketInboundQueue::new(max_pending_frames, max_frame_bytes),
            owner,
            allocations: VecDeque::new(),
            cancelled: false,
        }
    }

    /// Queues a decoded frame only after reserving its payload bytes.
    pub(crate) fn push(
        &mut self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
        frame: VmWebSocketFrame,
    ) -> Result<(), VmAccountedWebSocketQueueError> {
        if self.cancelled {
            return Err(VmAccountedWebSocketQueueError::Queue(
                "error[vm_websocket_queue]: queue is cancelled".to_string(),
            ));
        }
        let frame_bytes = self
            .queue
            .validate_push(&frame)
            .map_err(VmAccountedWebSocketQueueError::Queue)?;
        let decision = memory
            .register_shared_allocation(
                processes,
                self.owner,
                VmSharedAllocationKind::ProtocolBuffer,
                frame_bytes,
            )
            .map_err(VmAccountedWebSocketQueueError::Memory)?;
        scheduler
            .charge_memory_reductions(processes, self.owner, frame_bytes)
            .map_err(VmAccountedWebSocketQueueError::Memory)?;
        if decision.pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Err(VmAccountedWebSocketQueueError::MemoryPressureRejected);
        }
        let allocation = decision.allocation_id.ok_or_else(|| {
            VmAccountedWebSocketQueueError::Memory(
                "accounted WebSocket enqueue did not produce an allocation id".to_string(),
            )
        })?;
        self.queue.queued_frame_bytes = self.queue.queued_frame_bytes.saturating_add(frame_bytes);
        self.queue.frames.push_back(frame);
        self.allocations.push_back(allocation);
        Ok(())
    }

    /// Pops one frame and releases its exact protocol-buffer ownership.
    pub(crate) fn pop(
        &mut self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
    ) -> Result<Option<VmWebSocketFrame>, VmAccountedWebSocketQueueError> {
        let Some(frame) = self.queue.frames.front() else {
            return Ok(None);
        };
        let frame_bytes = frame.payload_len();
        let allocation = self.allocations.front().copied().ok_or_else(|| {
            VmAccountedWebSocketQueueError::Memory(
                "accounted WebSocket queue is missing protocol-buffer ownership".to_string(),
            )
        })?;
        memory
            .release_shared_allocation(processes, allocation, self.owner)
            .map_err(VmAccountedWebSocketQueueError::Memory)?;
        scheduler
            .charge_memory_reductions(processes, self.owner, frame_bytes)
            .map_err(VmAccountedWebSocketQueueError::Memory)?;
        let frame = self
            .queue
            .frames
            .pop_front()
            .expect("accounted WebSocket frame was checked before release");
        self.allocations.pop_front();
        self.queue.queued_frame_bytes = self.queue.queued_frame_bytes.saturating_sub(frame_bytes);
        Ok(Some(frame))
    }

    /// Cancels the queue and atomically releases all pending frame buffers.
    pub(crate) fn cancel(
        &mut self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
    ) -> Result<usize, VmAccountedWebSocketQueueError> {
        let released_bytes = self.queue.frames.iter().try_fold(0usize, |total, frame| {
            total.checked_add(frame.payload_len()).ok_or_else(|| {
                VmAccountedWebSocketQueueError::Memory(
                    "accounted WebSocket cancellation byte size overflow".to_string(),
                )
            })
        })?;
        let allocations = self.allocations.iter().copied().collect::<Vec<_>>();
        let released = memory
            .release_shared_allocations(processes, &allocations, self.owner)
            .map_err(VmAccountedWebSocketQueueError::Memory)?;
        scheduler
            .charge_memory_reductions(processes, self.owner, released_bytes)
            .map_err(VmAccountedWebSocketQueueError::Memory)?;
        self.allocations.clear();
        self.queue.frames.clear();
        self.queue.queued_frame_bytes = 0;
        self.cancelled = true;
        Ok(released)
    }

    pub(crate) fn inspect(&self) -> VmWebSocketInboundQueueInfo {
        self.queue.inspect()
    }
}
