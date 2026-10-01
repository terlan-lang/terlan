use crate::session_bindings::SessionStorage;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

#[derive(Debug, Default)]
pub(crate) struct Probe {
    pub(crate) calls: Vec<Vec<String>>,
    pub(crate) fail: bool,
    pub(crate) value: Option<String>,
    pub(crate) live: bool,
}

impl Probe {
    pub(crate) fn record(&mut self, call: &[&str]) -> Result<(), BoundaryError> {
        self.calls
            .push(call.iter().map(|value| (*value).into()).collect());
        if self.fail {
            Err(failure())
        } else {
            Ok(())
        }
    }
}

pub(crate) fn failure() -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::VmRuntime,
        "test storage",
        "error[storage.closed]: unavailable",
    )
}

impl SessionStorage for Probe {
    fn current(&mut self, identity: &str) -> Result<String, BoundaryError> {
        self.record(&["current", identity])?;
        Ok("issued".into())
    }
    fn get(&mut self, identity: &str, key: &str) -> Result<Option<String>, BoundaryError> {
        self.record(&["get", identity, key])?;
        Ok(self.value.clone())
    }
    fn set(&mut self, identity: &str, key: &str, value: &str) -> Result<(), BoundaryError> {
        self.record(&["set", identity, key, value])
    }
    fn delete(&mut self, identity: &str, key: &str) -> Result<(), BoundaryError> {
        self.record(&["delete", identity, key])
    }
    fn rotate(&mut self, identity: &str) -> Result<String, BoundaryError> {
        self.record(&["rotate", identity])?;
        Ok("rotated".into())
    }
    fn expire(&mut self, identity: &str) -> Result<(), BoundaryError> {
        self.record(&["expire", identity])
    }
    fn is_live(&mut self, identity: &str) -> Result<bool, BoundaryError> {
        self.record(&["is_live", identity])?;
        Ok(self.live)
    }
}
