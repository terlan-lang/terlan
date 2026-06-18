//! In-process SafeNative runtime core for bridge workers.
//!
//! This module owns the smallest executable runtime surface before actual
//! BEAM/NIF wiring: decode stable bridge terms, dispatch the operation through
//! resource-backed adapters, and encode a stable reply term.

use crate::dispatch::dispatch_with_resources;
use crate::handle::SafeNativeHandle;
use crate::resource::{ResourceError, ResourceStore};
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
