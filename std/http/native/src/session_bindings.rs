//! Session argument contracts, independent of VM heaps and storage internals.

use terlan_runtime_abi::{
    BoundaryError, ErrorDomain, FromNativeValue, NativeContextBinding, NativeValue,
};

/// Storage supplied by the application host. Identities and keys are exact,
/// source-selected strings, not cookies or serialized session records.
pub trait SessionStorage {
    fn lookup(&mut self, identity: &str) -> Result<Option<String>, BoundaryError>;
    fn create(
        &mut self,
        excluded_identity: &str,
        ttl_seconds: u64,
    ) -> Result<String, BoundaryError>;
    fn get(&mut self, identity: &str, key: &str) -> Result<Option<String>, BoundaryError>;
    fn set(&mut self, identity: &str, key: &str, value: &str) -> Result<(), BoundaryError>;
    fn delete(&mut self, identity: &str, key: &str) -> Result<(), BoundaryError>;
    fn rotate(&mut self, identity: &str, ttl_seconds: u64) -> Result<String, BoundaryError>;
    fn expire(&mut self, identity: &str) -> Result<(), BoundaryError>;
    fn is_live(&mut self, identity: &str) -> Result<bool, BoundaryError>;
}

/// Exact package catalog supplied to an application's native-service registry.
/// Each callback checks every argument type before touching storage.
pub fn bindings<S: SessionStorage + ?Sized>() -> [NativeContextBinding<S>; 8] {
    [
        NativeContextBinding::new("std.http.session.lookup", 1, |storage: &mut S, args| {
            storage
                .lookup(<&str>::from_native(&args[0])?)
                .map(Into::into)
        }),
        NativeContextBinding::new("std.http.session.create", 2, |storage: &mut S, args| {
            storage
                .create(<&str>::from_native(&args[0])?, lifetime(&args[1])?)
                .map(Into::into)
        }),
        NativeContextBinding::new("std.http.session.get", 2, |storage: &mut S, args| {
            storage
                .get(
                    <&str>::from_native(&args[0])?,
                    <&str>::from_native(&args[1])?,
                )
                .map(Into::into)
        }),
        NativeContextBinding::new("std.http.session.set", 3, |storage: &mut S, args| {
            storage
                .set(
                    <&str>::from_native(&args[0])?,
                    <&str>::from_native(&args[1])?,
                    <&str>::from_native(&args[2])?,
                )
                .map(|()| NativeValue::Unit)
        }),
        NativeContextBinding::new("std.http.session.delete", 2, |storage: &mut S, args| {
            storage
                .delete(
                    <&str>::from_native(&args[0])?,
                    <&str>::from_native(&args[1])?,
                )
                .map(|()| NativeValue::Unit)
        }),
        NativeContextBinding::new("std.http.session.rotate", 2, |storage: &mut S, args| {
            storage
                .rotate(<&str>::from_native(&args[0])?, lifetime(&args[1])?)
                .map(Into::into)
        }),
        NativeContextBinding::new("std.http.session.expire", 1, |storage: &mut S, args| {
            storage
                .expire(<&str>::from_native(&args[0])?)
                .map(|()| NativeValue::Unit)
        }),
        NativeContextBinding::new("std.http.session.is_live", 1, |storage: &mut S, args| {
            storage
                .is_live(<&str>::from_native(&args[0])?)
                .map(Into::into)
        }),
    ]
}

fn lifetime(value: &NativeValue) -> Result<u64, BoundaryError> {
    let seconds = i64::from_native(value)?;
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
