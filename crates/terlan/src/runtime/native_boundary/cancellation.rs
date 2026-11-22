//! Cooperative cancellation shared by VM capability transport and adapters.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Cloneable request-scoped cancellation signal.
#[derive(Clone, Debug, Default)]
pub struct NativeBoundaryCancellationToken {
    cancelled: Arc<AtomicBool>,
    ancestors: Vec<Arc<AtomicBool>>,
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

    /// Creates an independently cancellable child that also observes this token.
    pub fn child(&self) -> Self {
        let mut ancestors = self.ancestors.clone();
        ancestors.push(Arc::clone(&self.cancelled));
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            ancestors,
        }
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self
                .ancestors
                .iter()
                .any(|ancestor| ancestor.load(Ordering::Acquire))
    }
}

#[cfg(test)]
#[path = "cancellation_test.rs"]
#[cfg(test)]
mod cancellation_test;
