//! Native call entry for general and admitted fixed-owner actors.

use super::*;

impl PureNativeBoundary {
    /// Starts a native call and returns while its actor remains parked on Yield.
    pub(crate) fn begin_call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, String> {
        self.begin_call_for_actor_with_projection(
            actors,
            context,
            function,
            args,
            NativeResultProjection::PublicValue,
            false,
        )
    }

    /// Starts a direct HTTP call whose non-file Response may bypass generic
    /// public-value materialization.
    pub(crate) fn begin_http_response_call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, String> {
        self.begin_call_for_actor_with_projection(
            actors,
            context,
            function,
            args,
            NativeResultProjection::HttpResponse,
            false,
        )
    }

    /// Starts an HTTP call on a service actor whose shard owns its lifecycle.
    pub(crate) fn begin_admitted_http_response_call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, String> {
        self.begin_call_for_actor_with_projection(
            actors,
            context,
            function,
            args,
            NativeResultProjection::HttpResponse,
            true,
        )
    }

    fn begin_call_for_actor_with_projection(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
        result_projection: NativeResultProjection,
        admitted_owner: bool,
    ) -> Result<PureNativeExecution, String> {
        let owner = context.actor();
        let mut prepared = self.prepare_call(
            context,
            function,
            args,
            actors.native_trace_enabled(),
            result_projection,
        )?;
        let trace_call = match prepared.trace_source.take() {
            Some(source) => actors.begin_native_trace_call(owner, source)?,
            None if admitted_owner => VmNativeTraceCall::disabled(),
            None => actors.begin_optional_native_trace_call(owner, None)?,
        };
        let reply = match self
            .backend
            .as_mut()
            .expect("prepare_call initializes the native backend")
            .call_frame(context, prepared.request_id, prepared.export_id, args)
        {
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
