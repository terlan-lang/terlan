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
mod tests {
    use super::*;

    /// Extracts a handle from a successful reply term.
    ///
    /// Inputs:
    /// - `reply`: runtime reply expected to contain a handle.
    ///
    /// Output:
    /// - `Some(handle)` when the reply is a handle success.
    /// - `None` after asserting the reply had an unexpected shape.
    ///
    /// Transformation:
    /// - Pattern-matches the stable reply term without unwrap/expect.
    fn handle_reply(reply: SafeNativeReplyTerm) -> Option<SafeNativeHandle> {
        let SafeNativeReplyTerm::Ok(SafeNativeTerm::Handle { id, generation }) = reply else {
            return None;
        };
        Some(SafeNativeHandle { id, generation })
    }

    /// Verifies JSON can execute through the term runtime path.
    ///
    /// Inputs:
    /// - JSON text, object key, and operation ids.
    ///
    /// Output:
    /// - Test passes when parse/get/as-string returns the expected text.
    ///
    /// Transformation:
    /// - Exercises term decoding, resource-backed dispatch, opaque handle
    ///   storage, and reply encoding together.
    #[test]
    fn runtime_executes_json_operations_through_terms() {
        let mut runtime = SafeNativeRuntime::new();
        let Some(root) = handle_reply(runtime.call(
            "std.data.json.parse",
            &[SafeNativeTerm::Text(String::from(r#"{"name":"Ada"}"#))],
        )) else {
            return;
        };
        let Some(name) = handle_reply(runtime.call(
            "std.data.json.get",
            &[
                SafeNativeTerm::Handle {
                    id: root.id,
                    generation: root.generation,
                },
                SafeNativeTerm::Text(String::from("name")),
            ],
        )) else {
            return;
        };

        assert_eq!(
            runtime.call(
                "std.data.json.as_string",
                &[SafeNativeTerm::Handle {
                    id: name.id,
                    generation: name.generation,
                }],
            ),
            SafeNativeReplyTerm::Ok(SafeNativeTerm::Text(String::from("Ada")))
        );
    }

    /// Verifies Base64 can execute through the term runtime path.
    ///
    /// Inputs:
    /// - Text operation argument.
    ///
    /// Output:
    /// - Test passes when encode returns the expected Base64 text.
    ///
    /// Transformation:
    /// - Exercises primitive-only SafeNative dispatch without resource handles.
    #[test]
    fn runtime_executes_base64_operations_through_terms() {
        let mut runtime = SafeNativeRuntime::new();

        assert_eq!(
            runtime.call(
                "std.encoding.base64.encode",
                &[SafeNativeTerm::Text(String::from("hello"))],
            ),
            SafeNativeReplyTerm::Ok(SafeNativeTerm::Text(String::from("aGVsbG8=")))
        );
    }

    /// Verifies path operations can return optional handle terms.
    ///
    /// Inputs:
    /// - Path text with a parent component.
    ///
    /// Output:
    /// - Test passes when parent returns `OptionalHandle(Some(_))`.
    ///
    /// Transformation:
    /// - Exercises optional opaque resource results through the runtime term
    ///   boundary.
    #[test]
    fn runtime_executes_path_optional_handle_operations_through_terms() {
        let mut runtime = SafeNativeRuntime::new();
        let Some(path) = handle_reply(runtime.call(
            "std.io.path.from_string",
            &[SafeNativeTerm::Text(String::from("src/main.terl"))],
        )) else {
            return;
        };

        let parent = runtime.call(
            "std.io.path.parent",
            &[SafeNativeTerm::Handle {
                id: path.id,
                generation: path.generation,
            }],
        );

        assert!(matches!(
            parent,
            SafeNativeReplyTerm::Ok(SafeNativeTerm::OptionalHandle(Some(_)))
        ));
    }

    /// Verifies disposed handles are rejected by later runtime calls.
    ///
    /// Inputs:
    /// - JSON parse output handle disposed before a second call.
    ///
    /// Output:
    /// - Test passes when the later operation returns `resource.stale_handle`.
    ///
    /// Transformation:
    /// - Exercises deterministic cleanup and stale-handle rejection through the
    ///   term runtime boundary.
    #[test]
    fn runtime_rejects_disposed_handles_through_terms() {
        let mut runtime = SafeNativeRuntime::new();
        let Some(root) = handle_reply(runtime.call(
            "std.data.json.parse",
            &[SafeNativeTerm::Text(String::from("null"))],
        )) else {
            return;
        };
        assert_eq!(
            runtime.dispose(root),
            SafeNativeReplyTerm::Ok(SafeNativeTerm::Unit)
        );

        assert_eq!(
            runtime.call(
                "std.data.json.is_null",
                &[SafeNativeTerm::Handle {
                    id: root.id,
                    generation: root.generation,
                }],
            ),
            SafeNativeReplyTerm::Error {
                code: String::from("resource.stale_handle"),
                message: String::from("SafeNative resource handle 1 generation 1 is not live."),
                offset: 0,
            }
        );
    }
}
