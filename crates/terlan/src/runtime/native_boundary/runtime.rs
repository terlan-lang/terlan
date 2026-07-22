//! In-process NativeBoundary runtime core for bridge workers.
//!
//! This module owns the smallest executable runtime surface before actual
//! VM/NIF wiring: decode stable bridge terms, dispatch the operation through
//! resource-backed adapters, and encode a stable reply term.

use crate::terlan_native::http;
use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;
use crate::terlan_native_boundary::dispatch::{
    dispatch_with_resources, dispatch_with_resources_for_process,
    dispatch_with_resources_for_process_with_capabilities,
    dispatch_with_resources_for_process_with_policy,
    dispatch_with_resources_for_process_with_policy_and_cancellation,
};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::metadata::NativeBoundaryWorkerClass;
use crate::terlan_native_boundary::resource::{
    ResourceError, ResourceStore, ResourceValue, SYSTEM_RESOURCE_OWNER,
};
use crate::terlan_native_boundary::runtime_events::{
    NativeBoundaryDispatchEvent, NativeBoundaryResourceEvent, NativeBoundaryResourceEventLog,
};
use crate::terlan_native_boundary::term::{
    decode_bridge_args, encode_dispatch_reply, NativeBoundaryReplyTerm, NativeBoundaryTerm,
};

/// NativeBoundary runtime state owned by one native worker.
#[derive(Debug, Default)]
pub struct NativeBoundaryRuntime {
    resources: ResourceStore,
    resource_events: NativeBoundaryResourceEventLog,
}

impl NativeBoundaryRuntime {
    /// Builds an empty NativeBoundary runtime.
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
            resource_events: NativeBoundaryResourceEventLog::default(),
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
    ///   code can pass only an opaque handle through the NativeBoundary term
    ///   boundary.
    pub fn register_http_request(
        &mut self,
        request: http::Request,
    ) -> Result<NativeBoundaryHandle, ResourceError> {
        self.register_http_request_for_process(SYSTEM_RESOURCE_OWNER, request)
    }

    /// Registers an HTTP request resource owned by one VM process.
    pub fn register_http_request_for_process(
        &mut self,
        owner_process_id: u64,
        request: http::Request,
    ) -> Result<NativeBoundaryHandle, ResourceError> {
        let handle = self
            .resources
            .insert_for_owner(owner_process_id, ResourceValue::HttpRequest(request))?;
        let reply = NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Handle {
            id: handle.id,
            generation: handle.generation,
        });
        self.resource_events.observe_call(
            owner_process_id,
            "runtime.http.register_request",
            &[],
            &reply,
        );
        Ok(handle)
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
        handle: NativeBoundaryHandle,
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
    pub fn http_response(
        &self,
        handle: NativeBoundaryHandle,
    ) -> Result<http::Response, ResourceError> {
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
    pub fn call(
        &mut self,
        operation: &str,
        args: &[NativeBoundaryTerm],
    ) -> NativeBoundaryReplyTerm {
        let decoded = decode_bridge_args(args);
        let reply = encode_dispatch_reply(dispatch_with_resources(
            &mut self.resources,
            operation,
            &decoded,
        ));
        self.resource_events
            .observe_call(SYSTEM_RESOURCE_OWNER, operation, args, &reply);
        reply
    }

    /// Calls one operation as a VM process with resource-owner enforcement.
    pub fn call_for_process(
        &mut self,
        caller_process_id: u64,
        operation: &str,
        args: &[NativeBoundaryTerm],
    ) -> NativeBoundaryReplyTerm {
        let decoded = decode_bridge_args(args);
        let reply = encode_dispatch_reply(dispatch_with_resources_for_process(
            &mut self.resources,
            caller_process_id,
            operation,
            &decoded,
        ));
        self.resource_events
            .observe_call(caller_process_id, operation, args, &reply);
        reply
    }

    /// Calls as a VM process after validating its NativeBoundary capabilities.
    pub fn call_for_process_with_capabilities(
        &mut self,
        caller_process_id: u64,
        granted_capabilities: &[&str],
        operation: &str,
        args: &[NativeBoundaryTerm],
    ) -> NativeBoundaryReplyTerm {
        let decoded = decode_bridge_args(args);
        let reply = encode_dispatch_reply(dispatch_with_resources_for_process_with_capabilities(
            &mut self.resources,
            caller_process_id,
            granted_capabilities,
            operation,
            &decoded,
        ));
        self.resource_events
            .observe_call(caller_process_id, operation, args, &reply);
        reply
    }

    /// Calls as a VM process with capability and scheduler-class admission.
    pub fn call_for_process_with_policy(
        &mut self,
        caller_process_id: u64,
        granted_capabilities: &[&str],
        admitted_worker_classes: &[NativeBoundaryWorkerClass],
        operation: &str,
        args: &[NativeBoundaryTerm],
    ) -> NativeBoundaryReplyTerm {
        let decoded = decode_bridge_args(args);
        let reply = encode_dispatch_reply(dispatch_with_resources_for_process_with_policy(
            &mut self.resources,
            caller_process_id,
            granted_capabilities,
            admitted_worker_classes,
            operation,
            &decoded,
        ));
        self.resource_events
            .observe_call(caller_process_id, operation, args, &reply);
        reply
    }

    /// Calls as a VM process while exposing cooperative cancellation to dispatch.
    pub fn call_for_process_with_policy_and_cancellation(
        &mut self,
        caller_process_id: u64,
        granted_capabilities: &[&str],
        admitted_worker_classes: &[NativeBoundaryWorkerClass],
        operation: &str,
        args: &[NativeBoundaryTerm],
        cancellation: &NativeBoundaryCancellationToken,
    ) -> NativeBoundaryReplyTerm {
        let decoded = decode_bridge_args(args);
        let reply = encode_dispatch_reply(
            dispatch_with_resources_for_process_with_policy_and_cancellation(
                &mut self.resources,
                caller_process_id,
                granted_capabilities,
                admitted_worker_classes,
                operation,
                &decoded,
                cancellation,
            ),
        );
        self.resource_events
            .observe_call(caller_process_id, operation, args, &reply);
        reply
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
    pub fn dispose(&mut self, handle: NativeBoundaryHandle) -> NativeBoundaryReplyTerm {
        self.dispose_for_process(SYSTEM_RESOURCE_OWNER, handle)
    }

    /// Disposes one resource as its owning VM process.
    pub fn dispose_for_process(
        &mut self,
        caller_process_id: u64,
        handle: NativeBoundaryHandle,
    ) -> NativeBoundaryReplyTerm {
        let reply = match self.resources.dispose_for_owner(handle, caller_process_id) {
            Ok(()) => NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit),
            Err(error) => resource_error_reply(error),
        };
        self.resource_events
            .record_dispose(caller_process_id, handle, &reply);
        reply
    }

    /// Returns bounded NativeBoundary resource events in lifecycle order.
    pub fn resource_events(&self) -> impl Iterator<Item = &NativeBoundaryResourceEvent> {
        self.resource_events.iter()
    }

    /// Returns manifest-correlated NativeBoundary dispatch events in lifecycle order.
    pub fn dispatch_events(&self) -> impl Iterator<Item = &NativeBoundaryDispatchEvent> {
        self.resource_events.dispatch_iter()
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
fn resource_error_reply(error: ResourceError) -> NativeBoundaryReplyTerm {
    NativeBoundaryReplyTerm::Error {
        code: error.code().to_string(),
        message: error.message().to_string(),
        offset: 0,
    }
}

#[cfg(test)]
#[path = "runtime_test.rs"]
mod runtime_test;
