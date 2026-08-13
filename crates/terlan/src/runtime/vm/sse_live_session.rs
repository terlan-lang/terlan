#[cfg(test)]
use super::VmSseEvent;
use super::{VmSseEndpointPlan, VmSseError, VmSseStream, VmSseStreamInfo};

/// Live SSE state admitted from one materialized router endpoint plan.
///
/// The immutable endpoint policy stays attached to the mutable stream for the
/// whole served-request lifetime. This prevents the production adapter from
/// validating one source plan and then opening an unrelated default stream.
#[derive(Debug)]
pub(crate) struct VmSseLiveSession {
    plan: VmSseEndpointPlan,
    stream: VmSseStream,
    open: bool,
}

impl VmSseLiveSession {
    /// Opens bounded stream state directly from an admitted endpoint plan.
    pub(crate) fn open(plan: VmSseEndpointPlan) -> Result<Self, VmSseError> {
        let stream = plan.open_stream()?;
        Ok(Self {
            plan,
            stream,
            open: true,
        })
    }

    /// Returns the source-selected endpoint policy retained by this session.
    pub(crate) fn plan(&self) -> &VmSseEndpointPlan {
        &self.plan
    }

    /// Returns the bounded queue state owned by this session.
    pub(crate) fn inspect(&self) -> VmSseStreamInfo {
        self.stream.inspect()
    }

    /// Queues one event under the endpoint's bounded stream policy.
    #[cfg(test)]
    pub(crate) fn enqueue(&mut self, event: VmSseEvent) -> Result<(), VmSseError> {
        self.stream.enqueue(event)
    }

    /// Encodes and removes the oldest event admitted to this stream.
    #[cfg(test)]
    pub(crate) fn flush_next(&mut self) -> Result<Option<Vec<u8>>, VmSseError> {
        self.stream.flush_next()
    }

    /// Returns whether the admitted session remains open.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Closes both the live-session lease and its bounded stream.
    #[cfg(test)]
    pub(crate) fn close(&mut self) {
        self.stream.close();
        self.open = false;
    }
}
