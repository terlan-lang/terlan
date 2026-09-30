//! Immutable SSE route policies, independent of VM and transport state.

use super::SseError;

/// Package-owned SSE endpoint policy with opaque callback identities.
///
/// Inputs:
/// - Maximum pending event count, maximum encoded event bytes, and optional
///   keep-alive interval.
///
/// Output:
/// - Validated route-level SSE policy consumed by the host stream adapter.
///
/// Transformation:
/// - Keeps router dispatch typed without storing live mutable stream state in the
///   route table itself.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct SseEndpointPlan<C> {
    max_pending_events: usize,
    max_event_bytes: usize,
    keep_alive_ms: Option<u64>,
    callbacks: Option<SseCallbacks<C>>,
}

impl<'de, C: serde::Deserialize<'de>> serde::Deserialize<'de> for SseEndpointPlan<C> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(bound(deserialize = "C: serde::Deserialize<'de>"))]
        struct Wire<C> {
            max_pending_events: usize,
            max_event_bytes: usize,
            keep_alive_ms: Option<u64>,
            callbacks: Option<SseCallbacks<C>>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let mut plan = Self::new(wire.max_pending_events, wire.max_event_bytes)
            .map_err(serde::de::Error::custom)?;
        if let Some(interval) = wire.keep_alive_ms {
            plan = plan
                .with_keep_alive_ms(interval)
                .map_err(serde::de::Error::custom)?;
        }
        // A new plan has no callbacks to conflict with the deserialized set.
        plan.callbacks = wire.callbacks;
        Ok(plan)
    }
}

/// Complete callback set for one SSE endpoint.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SseCallbacks<C> {
    /// Called after stream admission.
    pub open: C,
    /// Called when one application event becomes ready.
    pub event_ready: C,
    /// Called when the VM emits a keep-alive comment.
    pub keep_alive: C,
    /// Called before graceful stream drain and close.
    pub drain: C,
    /// Called during abrupt scheduler or transport cancellation.
    pub cancellation: C,
}

impl<C> SseEndpointPlan<C> {
    /// Transforms callback ownership without changing validated endpoint policy.
    pub fn map_callbacks<D>(self, mut map: impl FnMut(C) -> D) -> SseEndpointPlan<D> {
        SseEndpointPlan {
            max_pending_events: self.max_pending_events,
            max_event_bytes: self.max_event_bytes,
            keep_alive_ms: self.keep_alive_ms,
            callbacks: self.callbacks.map(|callbacks| SseCallbacks {
                open: map(callbacks.open),
                event_ready: map(callbacks.event_ready),
                keep_alive: map(callbacks.keep_alive),
                drain: map(callbacks.drain),
                cancellation: map(callbacks.cancellation),
            }),
        }
    }

    /// Creates an SSE endpoint plan with explicit non-zero stream limits.
    pub fn new(max_pending_events: usize, max_event_bytes: usize) -> Result<Self, SseError> {
        if max_pending_events == 0 || max_event_bytes == 0 {
            return Err(SseError::BackpressureExceeded);
        }
        Ok(Self {
            max_pending_events,
            max_event_bytes,
            keep_alive_ms: None,
            callbacks: None,
        })
    }

    /// Adds a non-zero keep-alive interval in milliseconds.
    pub fn with_keep_alive_ms(mut self, keep_alive_ms: u64) -> Result<Self, SseError> {
        if keep_alive_ms == 0 {
            return Err(SseError::InvalidKeepAlive);
        }
        self.keep_alive_ms = Some(keep_alive_ms);
        Ok(self)
    }

    /// Returns the maximum queued event count for streams opened from this plan.
    pub fn max_pending_events(&self) -> usize {
        self.max_pending_events
    }

    /// Returns the maximum encoded event size for streams opened from this plan.
    pub fn max_event_bytes(&self) -> usize {
        self.max_event_bytes
    }

    /// Returns the optional keep-alive interval in milliseconds.
    pub fn keep_alive_ms(&self) -> Option<u64> {
        self.keep_alive_ms
    }

    /// Attaches one complete callback set to this endpoint.
    pub fn with_callbacks(mut self, callbacks: SseCallbacks<C>) -> Result<Self, SseError> {
        if self.callbacks.is_some() {
            return Err(SseError::CallbacksAlreadyConfigured);
        }
        self.callbacks = Some(callbacks);
        Ok(self)
    }

    /// Returns the generated callback set retained by this endpoint.
    pub fn callbacks(&self) -> Option<&SseCallbacks<C>> {
        self.callbacks.as_ref()
    }
}
