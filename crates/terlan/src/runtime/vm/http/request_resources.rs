use std::collections::BTreeMap;

use crate::runtime::vm::process::VmProcessId;

/// Transient resource metrics for VM HTTP handler dispatch.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmHttpRequestResourceMetrics {
    pub(crate) active_body_buffers: usize,
    pub(crate) active_telemetry_spans: usize,
    pub(crate) active_route_contexts: usize,
    pub(crate) active_body_bytes: usize,
    pub(crate) peak_body_buffers: usize,
    pub(crate) peak_telemetry_spans: usize,
    pub(crate) peak_route_contexts: usize,
    pub(crate) peak_body_bytes: usize,
    pub(crate) completed_requests: usize,
    pub(crate) last_request_id: u64,
}

/// Stable ownership evidence for a request resource left active at shutdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpRequestResourceLeak {
    pub(crate) owner: VmProcessId,
    pub(crate) request_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmHttpRequestResources {
    request_id: u64,
    body_bytes: usize,
}

/// VM-owned lifecycle for transient HTTP request resources.
#[derive(Debug, Default)]
pub(crate) struct VmHttpRequestResourceTracker {
    next_request_id: u64,
    active: BTreeMap<VmProcessId, VmHttpRequestResources>,
    metrics: VmHttpRequestResourceMetrics,
}

impl VmHttpRequestResourceTracker {
    pub(crate) fn begin(&mut self, owner: VmProcessId, body_bytes: usize) -> Result<u64, String> {
        if let Some(active) = self.active.get(&owner) {
            return Err(format!(
                "VM HTTP process {} already owns request resources for request {}",
                owner.as_u64(),
                active.request_id
            ));
        }
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| "VM HTTP request id overflow".to_string())?;
        let request_id = self.next_request_id;
        self.active.insert(
            owner,
            VmHttpRequestResources {
                request_id,
                body_bytes,
            },
        );
        self.refresh_active_metrics();
        self.metrics.peak_body_buffers = self
            .metrics
            .peak_body_buffers
            .max(self.metrics.active_body_buffers);
        self.metrics.peak_telemetry_spans = self
            .metrics
            .peak_telemetry_spans
            .max(self.metrics.active_telemetry_spans);
        self.metrics.peak_route_contexts = self
            .metrics
            .peak_route_contexts
            .max(self.metrics.active_route_contexts);
        self.metrics.peak_body_bytes = self
            .metrics
            .peak_body_bytes
            .max(self.metrics.active_body_bytes);
        self.metrics.last_request_id = request_id;
        Ok(request_id)
    }

    pub(crate) fn finish(&mut self, owner: VmProcessId, request_id: u64) -> Result<(), String> {
        let active = self.active.get(&owner).ok_or_else(|| {
            format!(
                "VM HTTP process {} has no active request resources",
                owner.as_u64()
            )
        })?;
        if active.request_id != request_id {
            return Err(format!(
                "VM HTTP process {} request resource mismatch: expected {}, observed {request_id}",
                owner.as_u64(),
                active.request_id
            ));
        }
        self.active.remove(&owner);
        self.metrics.completed_requests = self.metrics.completed_requests.saturating_add(1);
        self.refresh_active_metrics();
        Ok(())
    }

    pub(crate) fn metrics(&self) -> VmHttpRequestResourceMetrics {
        self.metrics.clone()
    }

    pub(crate) fn leaks(&self) -> Vec<VmHttpRequestResourceLeak> {
        self.active
            .iter()
            .map(|(owner, resources)| VmHttpRequestResourceLeak {
                owner: *owner,
                request_id: resources.request_id,
            })
            .collect()
    }

    fn refresh_active_metrics(&mut self) {
        self.metrics.active_body_buffers = self.active.len();
        self.metrics.active_telemetry_spans = self.active.len();
        self.metrics.active_route_contexts = self.active.len();
        self.metrics.active_body_bytes = self.active.values().map(|row| row.body_bytes).sum();
    }
}
