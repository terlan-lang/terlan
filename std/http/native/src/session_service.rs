//! Application-lifetime session contexts, independent of compiler and VM types.

use std::sync::{Arc, Mutex, TryLockError};
use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeServices};

use crate::session_bindings::{bindings, SessionStorage};

/// Resource hosts provide clock attachment and bounded reclamation in addition
/// to storage. Cleanup must be nonblocking once the context lock is acquired.
pub trait SessionHost: SessionStorage + Send {
    fn start_clock(&mut self) -> Result<(), BoundaryError>;
    fn maintain(&mut self, limit: usize) -> Result<(), BoundaryError>;
}

/// One application's session context, shared by disposable handler images.
#[derive(Debug)]
pub struct SessionService<S> {
    storage: Arc<Mutex<S>>,
}

impl<S> Clone for SessionService<S> {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
        }
    }
}

impl<S: SessionHost + 'static> SessionService<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage: Arc::new(Mutex::new(storage)),
        }
    }

    /// Each image receives explicit grants to the same application context.
    pub fn native_services(&self) -> Result<NativeServices, BoundaryError> {
        let mut services = NativeServices::default();
        services.register_context(Arc::clone(&self.storage), bindings())?;
        Ok(services)
    }

    pub fn start_clock(&self) -> Result<(), BoundaryError> {
        self.with_storage(SessionHost::start_clock)?
    }

    /// Busy contexts are retried on the owner's next scheduled tick. This must
    /// never wait for a request while executing on a protocol owner.
    pub fn maintain(&self, limit: usize) -> Result<(), BoundaryError> {
        match self.storage.try_lock() {
            Ok(mut storage) => storage.maintain(limit),
            Err(TryLockError::WouldBlock) => Ok(()),
            Err(TryLockError::Poisoned(_)) => Err(lock_error()),
        }
    }

    /// Host inspection and setup share the same synchronization as native calls.
    /// Unlike maintenance, this operation may wait; do not use it on I/O owners.
    pub fn with_storage<T>(&self, operation: impl FnOnce(&mut S) -> T) -> Result<T, BoundaryError> {
        let mut storage = self.storage.lock().map_err(|_| lock_error())?;
        Ok(operation(&mut storage))
    }
}

fn lock_error() -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::VmRuntime,
        "HTTP session service",
        "HTTP session service lock poisoned",
    )
}

#[cfg(test)]
#[path = "session_service_test.rs"]
mod tests;
