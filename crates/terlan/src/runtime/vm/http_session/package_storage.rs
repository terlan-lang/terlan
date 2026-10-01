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
    fn current(&mut self, identity: &str) -> Result<String, BoundaryError> {
        super::current(self, Some(identity)).map(|lookup| lookup.session.id)
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

    fn rotate(&mut self, identity: &str) -> Result<String, BoundaryError> {
        super::rotate(self, &VmHttpSession::from_managed_id(identity.into()))
            .map(|lookup| lookup.session.id)
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
