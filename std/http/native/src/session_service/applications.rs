//! Application identity is supplied by the host; session lifetime is package-owned.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use super::{SessionHost, SessionService};

/// Keeps session contexts alive across disposable handler image generations.
pub struct ApplicationSessions<K, S> {
    services: Mutex<HashMap<K, SessionService<S>>>,
}

impl<K, S> Default for ApplicationSessions<K, S> {
    fn default() -> Self {
        Self {
            services: Mutex::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash, S: SessionHost + 'static> ApplicationSessions<K, S> {
    /// Starts exactly one context clock per key. Failed initialization is not
    /// published and can be retried. The factory must not reenter this registry.
    pub fn get_or_create(
        &self,
        key: K,
        create: impl FnOnce() -> S,
    ) -> Result<SessionService<S>, BoundaryError> {
        // Image admission is not a per-request operation. One lock keeps
        // construction and publication atomic without a read/write retry path.
        let mut services = self.services.lock().map_err(|_| cache_error())?;
        if let Some(service) = services.get(&key) {
            return Ok(service.clone());
        }
        let service = SessionService::new(create());
        service.start_clock()?;
        services.insert(key, service.clone());
        Ok(service)
    }
}

fn cache_error() -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::CommandExecution,
        "load VM HTTP session service",
        "error[serve.session_cache]: session service lock poisoned",
    )
}

#[cfg(test)]
mod tests;
