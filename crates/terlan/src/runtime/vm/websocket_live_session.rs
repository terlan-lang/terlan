#[cfg(test)]
use super::VmWebSocketFrame;
use super::{VmWebSocketEndpointPlan, VmWebSocketInboundQueue, VmWebSocketInboundQueueInfo};

/// Live WebSocket state admitted from one materialized router endpoint plan.
///
/// Keeping the plan and bounded inbound queue together makes endpoint limits
/// part of session ownership instead of metadata used only during validation.
#[derive(Debug)]
pub(crate) struct VmWebSocketLiveSession {
    plan: VmWebSocketEndpointPlan,
    inbound: VmWebSocketInboundQueue,
    open: bool,
}

impl VmWebSocketLiveSession {
    /// Opens bounded inbound state directly from an admitted endpoint plan.
    pub(crate) fn open(plan: VmWebSocketEndpointPlan) -> Self {
        let inbound = plan.open_inbound_queue();
        Self {
            plan,
            inbound,
            open: true,
        }
    }

    /// Returns the source-selected endpoint policy retained by this session.
    pub(crate) fn plan(&self) -> &VmWebSocketEndpointPlan {
        &self.plan
    }

    /// Returns the bounded inbound queue state owned by this session.
    pub(crate) fn inspect(&self) -> VmWebSocketInboundQueueInfo {
        self.inbound.inspect()
    }

    /// Queues one decoded frame under the endpoint's bounded pressure policy.
    #[cfg(test)]
    pub(crate) fn enqueue_inbound(&mut self, frame: VmWebSocketFrame) -> Result<(), String> {
        self.inbound.push(frame)
    }

    /// Removes the oldest admitted frame for generated callback dispatch.
    #[cfg(test)]
    pub(crate) fn next_inbound(&mut self) -> Option<VmWebSocketFrame> {
        self.inbound.pop()
    }

    /// Returns whether the admitted session remains open.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Ends the live-session lease after transport cleanup.
    #[cfg(test)]
    pub(crate) fn close(&mut self) {
        self.open = false;
    }
}
