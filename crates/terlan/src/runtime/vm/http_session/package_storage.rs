//! Adapter retained while session storage moves to package-owned actors.

use terlan_http_native::session_bindings::SessionStorage;
use terlan_http_native::session_service::SessionHost;
use terlan_runtime_abi::BoundaryError;

#[cfg(test)]
use super::VmHttpSessionService;
use super::{VmHttpSession, VmHttpSessionRuntime};

#[cfg(test)]
#[path = "package_storage_test.rs"]
mod tests;

impl SessionHost for VmHttpSessionRuntime {
    fn start_clock(&mut self) -> Result<(), BoundaryError> {
        self.sessions.start_clock();
        Ok(())
    }

    fn maintain(&mut self, limit: usize) -> Result<(), BoundaryError> {
        self.expire_due_limit(limit).map(|_| ()).map_err(|error| {
            BoundaryError::message(
                terlan_runtime_abi::ErrorDomain::VmRuntime,
                "expire HTTP session resources",
                error.to_string(),
            )
        })
    }
}

impl SessionStorage for VmHttpSessionRuntime {
    fn lookup(&mut self, identity: &str) -> Result<Option<String>, BoundaryError> {
        self.lookup_available(identity)
            .map(|lookup| lookup.map(|lookup| lookup.session.id))
            .map_err(session_error)
    }

    fn create(
        &mut self,
        excluded_identity: &str,
        ttl_seconds: u64,
    ) -> Result<String, BoundaryError> {
        self.create_session_for(excluded_identity, ttl_seconds)
            .map(|lookup| lookup.session.id)
            .map_err(session_error)
    }

    fn get(&mut self, identity: &str, key: &str) -> Result<Option<String>, BoundaryError> {
        super::get(self, &VmHttpSession::from_managed_id(identity.into()), key)
    }

    fn set(&mut self, identity: &str, key: &str, value: &str) -> Result<(), BoundaryError> {
        super::set(
            self,
            &VmHttpSession::from_managed_id(identity.into()),
            key,
            value,
        )
    }

    fn delete(&mut self, identity: &str, key: &str) -> Result<(), BoundaryError> {
        super::delete(self, &VmHttpSession::from_managed_id(identity.into()), key)
    }

    fn rotate(&mut self, identity: &str, ttl_seconds: u64) -> Result<String, BoundaryError> {
        self.rotate_for(
            &VmHttpSession::from_managed_id(identity.into()),
            ttl_seconds,
        )
        .map(|lookup| lookup.session.id)
        .map_err(session_error)
    }

    fn expire(&mut self, identity: &str) -> Result<(), BoundaryError> {
        super::expire(self, &VmHttpSession::from_managed_id(identity.into()))
    }

    fn is_live(&mut self, identity: &str) -> Result<bool, BoundaryError> {
        Ok(VmHttpSessionRuntime::is_live(
            self,
            &VmHttpSession::from_managed_id(identity.into()),
        ))
    }
}

fn session_error(error: String) -> BoundaryError {
    BoundaryError::message(
        terlan_runtime_abi::ErrorDomain::VmRuntime,
        "resolve HTTP session",
        error,
    )
}
