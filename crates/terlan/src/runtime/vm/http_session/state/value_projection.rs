//! Projects managed session values through the public runtime boundary.

use super::{ReplValue, VmHttpSession, VmHttpSessionRuntime};

/// Reads one string session value through the VM session runtime.
pub fn get(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
    key: &str,
) -> Result<Option<String>, terlan_runtime_abi::BoundaryError> {
    runtime
        .read(session, key)
        .map(|value| {
            value.map(|stored| match stored {
                ReplValue::String(value) => value,
                other => other.render(),
            })
        })
        .map_err(|error| {
            terlan_runtime_abi::BoundaryError::message(
                terlan_runtime_abi::ErrorDomain::VmRuntime,
                "read HTTP session value",
                error,
            )
        })
}
