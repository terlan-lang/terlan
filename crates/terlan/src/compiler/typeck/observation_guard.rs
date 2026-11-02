//! Shared restoration guard for temporary compiler observation scopes.

/// Restores the prior thread-local observation state when a scope exits.
pub(super) struct ObservationGuard {
    restore: Option<Box<dyn FnOnce()>>,
}

impl ObservationGuard {
    /// Creates a guard that runs the supplied restoration action exactly once.
    pub(super) fn new(restore: impl FnOnce() + 'static) -> Self {
        Self {
            restore: Some(Box::new(restore)),
        }
    }
}

impl Drop for ObservationGuard {
    fn drop(&mut self) {
        if let Some(restore) = self.restore.take() {
            restore();
        }
    }
}
