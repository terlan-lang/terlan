//! In-process SafeNative runtime core for bridge workers.
//!
//! This module owns the smallest executable runtime surface before actual
//! BEAM/NIF wiring: decode stable bridge terms, dispatch the operation through
//! resource-backed adapters, and encode a stable reply term.

use crate::dispatch::dispatch_with_resources;
use crate::handle::SafeNativeHandle;
use crate::http;
use crate::resource::{ResourceError, ResourceStore, ResourceValue};
use crate::term::{decode_bridge_args, encode_dispatch_reply, SafeNativeReplyTerm, SafeNativeTerm};

/// SafeNative runtime state owned by one native worker.
#[derive(Debug, Default)]
pub struct SafeNativeRuntime {
    resources: ResourceStore,
}

impl SafeNativeRuntime {
    /// Builds an empty SafeNative runtime.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Runtime with an empty resource store.
    ///
    /// Transformation:
    /// - Initializes deterministic resource ownership state for one worker.
    pub fn new() -> Self {
        Self {
            resources: ResourceStore::new(),
        }
    }

    /// Registers a server-owned HTTP request resource.
    ///
    /// Inputs:
    /// - `request`: request snapshot produced by a Rust HTTP server adapter.
    ///
    /// Output:
    /// - `Ok(handle)` for the stored request.
    /// - `Err(ResourceError)` if resource id allocation fails.
    ///
    /// Transformation:
    /// - Moves the request into the runtime resource store so handler bridge
    ///   code can pass only an opaque handle through the SafeNative term
    ///   boundary.
    pub fn register_http_request(
        &mut self,
        request: http::Request,
    ) -> Result<SafeNativeHandle, ResourceError> {
        self.resources.insert(ResourceValue::HttpRequest(request))
    }

    /// Returns recorded response-cookie mutations for a cookie jar resource.
    ///
    /// Inputs:
    /// - `handle`: opaque handle returned by `std.http.request.cookies`.
    ///
    /// Output:
    /// - `Ok(headers)` with serialized `Set-Cookie` values in mutation order.
    /// - `Err(ResourceError)` when the handle is stale or not a cookie jar.
    ///
    /// Transformation:
    /// - Validates the runtime resource handle, reads the adapter-owned jar,
    ///   and clones response metadata so the HTTP writer can apply it after a
    ///   Terlan handler returns its response.
    pub fn http_cookie_mutations(
        &self,
        handle: SafeNativeHandle,
    ) -> Result<Vec<String>, ResourceError> {
        self.resources
            .http_cookie_jar(handle)
            .map(|jar| jar.mutations().to_vec())
    }

    /// Returns a server-owned HTTP response resource snapshot.
    ///
    /// Inputs:
    /// - `handle`: opaque handle returned by `std.http.response.*`.
    ///
    /// Output:
    /// - `Ok(response)` with the portable response metadata and body.
    /// - `Err(ResourceError)` when the handle is stale or not a response.
    ///
    /// Transformation:
    /// - Validates the runtime resource handle and clones the response so an
    ///   HTTP server adapter can serialize it after handler execution.
    pub fn http_response(&self, handle: SafeNativeHandle) -> Result<http::Response, ResourceError> {
        self.resources.http_response(handle).cloned()
    }

    /// Calls one operation through the stable term boundary.
    ///
    /// Inputs:
    /// - `operation`: compiler-native operation id.
    /// - `args`: stable bridge terms supplied by a worker boundary.
    ///
    /// Output:
    /// - Stable reply term containing either a successful term result or stable
    ///   error fields.
    ///
    /// Transformation:
    /// - Decodes terms into bridge values, dispatches through the shared
    ///   resource-backed adapter surface, stores opaque outputs in the runtime
    ///   resource registry, and encodes the result back into a reply term.
    pub fn call(&mut self, operation: &str, args: &[SafeNativeTerm]) -> SafeNativeReplyTerm {
        let decoded = decode_bridge_args(args);
        encode_dispatch_reply(dispatch_with_resources(
            &mut self.resources,
            operation,
            &decoded,
        ))
    }

    /// Disposes one opaque resource handle.
    ///
    /// Inputs:
    /// - `handle`: resource handle previously returned by `call`.
    ///
    /// Output:
    /// - `Ok(Unit)` reply when disposal succeeds.
    /// - Stable error reply when the handle is stale or mismatched.
    ///
    /// Transformation:
    /// - Delegates ownership cleanup to `ResourceStore` and maps resource
    ///   errors into the same reply shape used by operation calls.
    pub fn dispose(&mut self, handle: SafeNativeHandle) -> SafeNativeReplyTerm {
        match self.resources.dispose(handle) {
            Ok(()) => SafeNativeReplyTerm::Ok(SafeNativeTerm::Unit),
            Err(error) => resource_error_reply(error),
        }
    }
}

/// Maps a resource-store error into a stable reply term.
///
/// Inputs:
/// - `error`: resource ownership error from `ResourceStore`.
///
/// Output:
/// - Stable error reply with code, message, and offset.
///
/// Transformation:
/// - Converts resource-store diagnostics into the same term-level error shape
///   used by dispatch failures.
fn resource_error_reply(error: ResourceError) -> SafeNativeReplyTerm {
    SafeNativeReplyTerm::Error {
        code: error.code().to_string(),
        message: error.message().to_string(),
        offset: 0,
    }
}

#[cfg(test)]
#[path = "runtime_test.rs"]
mod runtime_test;
