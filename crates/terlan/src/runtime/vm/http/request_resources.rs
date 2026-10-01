//! VM identity and diagnostic adaptation for package-owned HTTP accounting.

use crate::runtime::vm::process::VmProcessId;
use terlan_http_native::request_resources::{RequestResourceError, RequestResourceTracker};

pub(crate) use terlan_http_native::request_resources::RequestResourceMetrics as VmHttpRequestResourceMetrics;
pub(crate) type VmHttpRequestResourceLeak =
    terlan_http_native::request_resources::RequestResourceLeak<VmProcessId>;

#[derive(Debug, Default)]
pub(crate) struct VmHttpRequestResourceTracker {
    pub(super) inner: RequestResourceTracker<VmProcessId>,
}

impl VmHttpRequestResourceTracker {
    #[cfg(test)]
    pub(crate) fn begin(&mut self, owner: VmProcessId, body_bytes: usize) -> Result<u64, String> {
        self.inner.begin(owner, body_bytes).map_err(resource_error)
    }

    #[cfg(test)]
    pub(crate) fn finish(&mut self, owner: VmProcessId, request_id: u64) -> Result<(), String> {
        self.inner.finish(owner, request_id).map_err(resource_error)
    }

    pub(crate) fn metrics(&self) -> VmHttpRequestResourceMetrics {
        self.inner.metrics()
    }

    pub(crate) fn leaks(&self) -> Vec<VmHttpRequestResourceLeak> {
        self.inner.leaks()
    }
}

pub(super) fn resource_error(error: RequestResourceError<VmProcessId>) -> String {
    match error {
        RequestResourceError::AlreadyActive { owner, request_id } => format!(
            "VM HTTP process {} already owns request resources for request {request_id}",
            owner.as_u64()
        ),
        RequestResourceError::UnknownOwner { owner } => format!(
            "VM HTTP process {} has no active request resources", owner.as_u64()
        ),
        RequestResourceError::StaleRequest { owner, expected, observed } => format!(
            "VM HTTP process {} request resource mismatch: expected {expected}, observed {observed}",
            owner.as_u64()
        ),
        RequestResourceError::RequestIdOverflow => "VM HTTP request id overflow".into(),
        RequestResourceError::BodyBytesOverflow => "VM HTTP request body byte count overflow".into(),
    }
}
