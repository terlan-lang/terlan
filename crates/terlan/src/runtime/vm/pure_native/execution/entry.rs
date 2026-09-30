//! Native call entry for general and admitted fixed-owner actors.

use super::super::NativeCallTarget;
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
        self.begin_target_for_actor(
            actors,
            context,
            &NativeCallTarget::Export(function.into()),
            args,
            false,
        )
        .map_err(Into::into)
    }

    /// Starts a call on a service actor whose shard owns its lifecycle.
    pub(crate) fn begin_admitted_call_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        function: &str,
        args: &[ReplValue],
    ) -> Result<PureNativeExecution, String> {
        self.begin_target_for_actor(
            actors,
            context,
            &NativeCallTarget::Export(function.into()),
            args,
            true,
        )
        .map_err(Into::into)
    }

    pub(in crate::runtime::vm::pure_native) fn begin_target_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        target: &NativeCallTarget<'_>,
        args: &[ReplValue],
        admitted_owner: bool,
    ) -> crate::runtime::vm::VmRuntimeResult<PureNativeExecution> {
        let owner = context.actor();
        let mut prepared =
            self.prepare_target_call(context, target, args, actors.native_trace_enabled())?;
        let trace_call = match prepared.trace_source.take() {
            Some(source) => actors.begin_native_trace_call(owner, source)?,
            None if admitted_owner => VmNativeTraceCall::disabled(),
            None => actors.begin_optional_native_trace_call(owner, None)?,
        };
        let backend = self
            .backend
            .as_mut()
            .expect("call preparation requires a native backend");
        let result = match target {
            NativeCallTarget::Export(_) => backend
                .call_frame(context, prepared.request_id, prepared.export_id, args)
                .map_err(crate::runtime::vm::VmRuntimeError::from),
            NativeCallTarget::Closure(closure) => {
                backend.closure_frame(context, prepared.request_id, closure, args)
            }
        };
        let reply = match result {
            Ok(reply) => reply,
            Err(error) => {
                let _ = actors.fail_native_trace_call(owner, trace_call, error.to_string());
                return Err(error);
            }
        };
        if matches!(reply, TvmControlFrame::Transition { .. }) {
            prepared.continuations = Some(match target {
                NativeCallTarget::Export(_) => self
                    .call_cache
                    .as_ref()
                    .expect("resolved export installs its continuation cache")
                    .continuations
                    .clone(),
                NativeCallTarget::Closure(_) => self
                    .artifact
                    .as_ref()
                    .expect("prepared closure retains its admitted image")
                    .continuations
                    .clone(),
            });
        }
        let backend = self
            .backend
            .as_deref()
            .expect("prepared native backend remains available");
        handle_reply(backend, actors, context, prepared, 0, trace_call, reply).map_err(Into::into)
    }
}
