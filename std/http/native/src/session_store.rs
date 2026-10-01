//! Session storage policy over generic actor-owned state supplied by the host.

use terlan_runtime_abi::{ActorStateStore, BoundaryError, ErrorDomain};

use crate::session_bindings::SessionStorage;
use crate::session_registry::{RecoveryPolicy, SessionError, SessionRegistry, SessionResources};
use crate::session_service::SessionHost;

/// Package lifecycle and string-state policy over opaque host resources.
/// This registry is not an executing source actor or a second VM scheduler.
pub struct SessionStore<R: ActorStateStore<String>> {
    registry: SessionRegistry<R::Handle>,
    resources: R,
}

impl<R: ActorStateStore<String>> SessionStore<R> {
    /// Includes expired entries retained for pending or failed resource cleanup.
    pub fn is_empty(&self) -> bool {
        self.registry.entries().is_empty()
    }

    pub fn new(
        resources: R,
        ttl_seconds: u64,
        recovery: RecoveryPolicy,
    ) -> Result<Self, SessionError> {
        Ok(Self {
            registry: SessionRegistry::new(ttl_seconds, recovery)?,
            resources,
        })
    }

    /// Preserve the one-day lifetime and local replacement policy for serving.
    pub fn with_defaults(resources: R) -> Result<Self, SessionError> {
        Self::new(resources, 86_400, RecoveryPolicy::CreateLocalReplacement)
    }

    fn live(&mut self, identity: &str) -> Result<R::Handle, BoundaryError> {
        self.registry
            .live(identity, &mut Resources(&mut self.resources))
            .map(|entry| entry.value)
            .map_err(session_error)
    }
}

impl<R: ActorStateStore<String>> SessionHost for SessionStore<R> {
    fn start_clock(&mut self) -> Result<(), BoundaryError> {
        self.registry.start_clock();
        Ok(())
    }

    fn maintain(&mut self, limit: usize) -> Result<(), BoundaryError> {
        self.registry
            .expire_due_limit(&mut Resources(&mut self.resources), limit)
            .map(|_| ())
            .map_err(session_error)
    }
}

impl<R: ActorStateStore<String>> SessionStorage for SessionStore<R> {
    fn current(&mut self, identity: &str) -> Result<String, BoundaryError> {
        self.registry
            .acquire(Some(identity), &mut Resources(&mut self.resources))
            .map(|entry| entry.id)
            .map_err(session_error)
    }

    fn get(&mut self, identity: &str, key: &str) -> Result<Option<String>, BoundaryError> {
        let handle = self.live(identity)?;
        self.resources.read(&handle, key)
    }

    fn set(&mut self, identity: &str, key: &str, value: &str) -> Result<(), BoundaryError> {
        let handle = self.live(identity)?;
        self.resources.write(&handle, key, value.into())
    }

    fn delete(&mut self, identity: &str, key: &str) -> Result<(), BoundaryError> {
        let handle = self.live(identity)?;
        self.resources.delete(&handle, key)
    }

    fn rotate(&mut self, identity: &str) -> Result<String, BoundaryError> {
        self.registry
            .rotate(identity, &mut Resources(&mut self.resources))
            .map(|entry| entry.id)
            .map_err(session_error)
    }

    fn expire(&mut self, identity: &str) -> Result<(), BoundaryError> {
        self.registry
            .expire(identity, &mut Resources(&mut self.resources))
            .map_err(session_error)
    }

    fn is_live(&mut self, identity: &str) -> Result<bool, BoundaryError> {
        Ok(self
            .registry
            .is_live(identity, |handle| self.resources.check_live(handle).is_ok()))
    }
}

struct Resources<'a, R>(&'a mut R);

impl<R: ActorStateStore<String>> SessionResources<R::Handle> for Resources<'_, R> {
    fn create(&mut self, identity: &str) -> Result<R::Handle, SessionError> {
        self.0
            .create("std.http.Session", &format!("http_session:{identity}"))
            .map_err(resource_error)
    }

    fn failure(&self, _identity: &str, value: &R::Handle) -> Option<SessionError> {
        self.0.check_live(value).err().map(resource_error)
    }

    fn release(&mut self, value: &R::Handle) -> Result<(), SessionError> {
        self.0.release(value).map_err(resource_error)
    }
}

fn resource_error(error: BoundaryError) -> SessionError {
    SessionError::Resource(error.to_string())
}

fn session_error(error: SessionError) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "HTTP session store",
        error.to_string(),
    )
}

#[cfg(test)]
#[path = "session_store_test.rs"]
mod tests;
