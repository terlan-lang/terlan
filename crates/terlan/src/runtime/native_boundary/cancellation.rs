//! Cooperative cancellation shared by VM capability transport and adapters.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Cloneable request-scoped cancellation signal.
#[derive(Clone, Debug, Default)]
pub struct NativeBoundaryCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl NativeBoundaryCancellationToken {
    /// Creates a live cancellation token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the associated request as cancelled.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "cancellation_test.rs"]
#[cfg(test)]
mod cancellation_test;
