//! HTTP request accounting, parameterized by the host's opaque owner identity.

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequestResourceMetrics {
    pub active_body_buffers: usize,
    pub active_telemetry_spans: usize,
    pub active_route_contexts: usize,
    pub active_body_bytes: usize,
    pub peak_body_buffers: usize,
    pub peak_telemetry_spans: usize,
    pub peak_route_contexts: usize,
    pub peak_body_bytes: usize,
    pub completed_requests: usize,
    pub last_request_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestResourceLeak<O> {
    pub owner: O,
    pub request_id: u64,
}

/// Failures are structural; the host decides how to render its owner identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestResourceError<O> {
    AlreadyActive {
        owner: O,
        request_id: u64,
    },
    UnknownOwner {
        owner: O,
    },
    StaleRequest {
        owner: O,
        expected: u64,
        observed: u64,
    },
    RequestIdOverflow,
    BodyBytesOverflow,
}

#[derive(Debug)]
struct RequestResources {
    request_id: u64,
    body_bytes: usize,
}

/// Counts transient request resources, not physical allocations or VM handles.
/// Ownership and cleanup of actual buffers remain with the calling host.
#[derive(Debug)]
pub struct RequestResourceTracker<O> {
    next_request_id: u64,
    active: BTreeMap<O, RequestResources>,
    metrics: RequestResourceMetrics,
}

impl<O> Default for RequestResourceTracker<O> {
    fn default() -> Self {
        Self {
            next_request_id: 0,
            active: BTreeMap::new(),
            metrics: RequestResourceMetrics::default(),
        }
    }
}

impl<O: Copy + Ord> RequestResourceTracker<O> {
    /// Admits an owner once, checking all fallible counters before mutation.
    pub fn begin(&mut self, owner: O, body_bytes: usize) -> Result<u64, RequestResourceError<O>> {
        if let Some(active) = self.active.get(&owner) {
            return Err(RequestResourceError::AlreadyActive {
                owner,
                request_id: active.request_id,
            });
        }
        let request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or(RequestResourceError::RequestIdOverflow)?;
        let active_bytes = self
            .metrics
            .active_body_bytes
            .checked_add(body_bytes)
            .ok_or(RequestResourceError::BodyBytesOverflow)?;
        self.active.insert(
            owner,
            RequestResources {
                request_id,
                body_bytes,
            },
        );
        self.next_request_id = request_id;
        self.metrics.active_body_bytes = active_bytes;
        self.refresh_active_counts();
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
        self.metrics.peak_body_bytes = self.metrics.peak_body_bytes.max(active_bytes);
        self.metrics.last_request_id = request_id;
        Ok(request_id)
    }

    /// A stale completion cannot release a later request belonging to this owner.
    pub fn finish(&mut self, owner: O, request_id: u64) -> Result<(), RequestResourceError<O>> {
        let active = self
            .active
            .get(&owner)
            .ok_or(RequestResourceError::UnknownOwner { owner })?;
        if active.request_id != request_id {
            return Err(RequestResourceError::StaleRequest {
                owner,
                expected: active.request_id,
                observed: request_id,
            });
        }
        self.metrics.active_body_bytes -= active.body_bytes;
        self.active.remove(&owner);
        self.metrics.completed_requests = self.metrics.completed_requests.saturating_add(1);
        self.refresh_active_counts();
        Ok(())
    }

    pub fn metrics(&self) -> RequestResourceMetrics {
        self.metrics.clone()
    }

    pub fn leaks(&self) -> Vec<RequestResourceLeak<O>> {
        self.active
            .iter()
            .map(|(owner, resources)| RequestResourceLeak {
                owner: *owner,
                request_id: resources.request_id,
            })
            .collect()
    }

    fn refresh_active_counts(&mut self) {
        self.metrics.active_body_buffers = self.active.len();
        self.metrics.active_telemetry_spans = self.active.len();
        self.metrics.active_route_contexts = self.active.len();
    }
}

#[cfg(test)]
#[path = "request_resources_test.rs"]
mod tests;
