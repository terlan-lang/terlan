//! Bounded application maintenance on an existing protocol owner, including idle time.

use std::sync::Arc;
use std::time::{Duration, Instant};
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

pub(crate) struct VmProtocolMaintenance {
    interval: Duration,
    next: Option<Instant>,
    callback: Arc<dyn Fn() -> Result<(), BoundaryError> + Send + Sync>,
}

impl VmProtocolMaintenance {
    /// Callbacks must be bounded and nonblocking. Failure is reported to the
    /// host while the next attempt remains scheduled; no retry busy-loop occurs.
    pub(crate) fn new(
        interval: Duration,
        callback: impl Fn() -> Result<(), BoundaryError> + Send + Sync + 'static,
    ) -> Result<Self, BoundaryError> {
        let now = Instant::now();
        if interval.is_zero() || now.checked_add(interval).is_none() {
            return Err(BoundaryError::message(
                ErrorDomain::VmRuntime,
                "protocol maintenance",
                "maintenance interval must be positive and representable",
            ));
        }
        Ok(Self {
            interval,
            next: Some(now),
            callback: Arc::new(callback),
        })
    }

    pub(super) fn run_due(&mut self, now: Instant) -> Result<(), BoundaryError> {
        if self.next.is_none_or(|next| now < next) {
            return Ok(());
        }
        self.next = now.checked_add(self.interval);
        if self.next.is_none() {
            return Err(BoundaryError::message(
                ErrorDomain::VmRuntime,
                "protocol maintenance",
                "maintenance deadline overflow",
            ));
        }
        (self.callback)()
    }

    pub(super) fn timeout(&self, now: Instant) -> Option<Duration> {
        self.next.map(|next| next.saturating_duration_since(now))
    }
}

pub(super) fn next_timeout(
    timer: Option<Duration>,
    maintenance: Option<Duration>,
) -> Option<Duration> {
    match (timer, maintenance) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
#[path = "maintenance_test.rs"]
mod tests;
