//! Session argument contracts, independent of VM heaps and storage internals.

use terlan_runtime_abi::{native_binding, BoundaryError, ErrorDomain, NativeContextBinding};

/// Storage supplied by the application host. Identities and keys are exact,
/// source-selected strings, not cookies or serialized session records.
pub trait SessionStorage {
    fn lookup(&mut self, identity: &str) -> Result<Option<String>, BoundaryError>;
    fn try_create(&mut self, identity: &str, ttl_seconds: u64) -> Result<bool, BoundaryError>;
    fn get(&mut self, identity: &str, key: &str) -> Result<Option<String>, BoundaryError>;
    fn set(&mut self, identity: &str, key: &str, value: &str) -> Result<(), BoundaryError>;
    fn delete(&mut self, identity: &str, key: &str) -> Result<(), BoundaryError>;
    fn try_rotate(
        &mut self,
        identity: &str,
        replacement: &str,
        ttl_seconds: u64,
    ) -> Result<bool, BoundaryError>;
    fn expire(&mut self, identity: &str) -> Result<(), BoundaryError>;
    fn is_live(&mut self, identity: &str) -> Result<bool, BoundaryError>;
}

/// Exact package catalog supplied to an application's native-service registry.
/// Each callback checks every argument type before touching storage.
pub fn bindings<S: SessionStorage + ?Sized>() -> [NativeContextBinding<S>; 8] {
    [
        native_binding!("std.http.session.lookup", |storage: &mut S; identity: &str| {
            storage.lookup(identity)
        }),
        native_binding!("std.http.session.try_create", |storage: &mut S; identity: &str, seconds: i64| {
            storage.try_create(identity, lifetime(seconds)?)
        }),
        native_binding!("std.http.session.get", |storage: &mut S; identity: &str, key: &str| {
            storage.get(identity, key)
        }),
        native_binding!("std.http.session.set", |storage: &mut S; identity: &str, key: &str, value: &str| {
            storage.set(identity, key, value)
        }),
        native_binding!("std.http.session.delete", |storage: &mut S; identity: &str, key: &str| {
            storage.delete(identity, key)
        }),
        native_binding!("std.http.session.try_rotate", |storage: &mut S; identity: &str, replacement: &str, seconds: i64| {
            storage.try_rotate(identity, replacement, lifetime(seconds)?)
        }),
        native_binding!("std.http.session.expire", |storage: &mut S; identity: &str| {
            storage.expire(identity)
        }),
        native_binding!("std.http.session.is_live", |storage: &mut S; identity: &str| {
            storage.is_live(identity)
        }),
    ]
}

fn lifetime(seconds: i64) -> Result<u64, BoundaryError> {
    u64::try_from(seconds)
        .ok()
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| {
            BoundaryError::message(
                ErrorDomain::NativeBoundary,
                "HTTP session lifetime",
                crate::session_registry::SessionError::ZeroTtl.to_string(),
            )
        })
}

#[cfg(test)]
#[path = "session_bindings_test.rs"]
mod tests;
