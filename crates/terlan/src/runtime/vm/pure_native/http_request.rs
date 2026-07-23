//! Typed projected HTTP ingress for an admitted native call.

use std::collections::HashSet;

use super::execution::{handle_reply, PureNativeExecution};
use super::{
    NativeResultProjection, PureNativeBoundary, PureNativeExecutionContext, RequestFieldProjection,
    RequestParts,
};
use crate::runtime::native_image::control::TvmControlFrame;
use crate::runtime::vm::actor::VmActorRuntime;

impl PureNativeBoundary {
    /// Starts a one-argument handler by admitting its compiler-proven Request
    /// projection directly into the actor heap.
    pub(super) fn begin_projected_http_request_call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        request: RequestParts,
        projection: RequestFieldProjection,
    ) -> Result<PureNativeExecution, String> {
        let owner = context.actor();
        let mut prepared = self.prepare_call_arity(
            context,
            function,
            1,
            actors.native_trace_enabled(),
            NativeResultProjection::HttpResponse,
        )?;
        let trace_call =
            actors.begin_optional_native_trace_call(owner, prepared.trace_source.take())?;
        let reply = match self
            .backend
            .as_mut()
            .expect("prepared call initializes the native backend")
            .call_projected_http_request_frame(
                context,
                prepared.request_id,
                prepared.export_id,
                request,
                projection,
            ) {
            Ok(reply) => reply,
            Err(error) => {
                let _ = actors.fail_native_trace_call(owner, trace_call, error.clone());
                return Err(error);
            }
        };
        if matches!(reply, TvmControlFrame::Transition { .. }) {
            prepared.continuations = Some(
                self.call_cache
                    .as_ref()
                    .expect("resolved export installs its continuation cache")
                    .continuations
                    .as_ref()
                    .to_vec(),
            );
        }
        let backend = self
            .backend
            .as_deref()
            .expect("prepared native backend remains available");
        handle_reply(
            backend,
            actors,
            context,
            prepared,
            HashSet::new(),
            trace_call,
            reply,
        )
    }
}
