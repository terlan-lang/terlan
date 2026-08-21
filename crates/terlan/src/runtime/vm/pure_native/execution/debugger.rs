use super::*;

impl PureNativeBoundary {
    /// Materializes one parked capture through its actor-owned heap.
    pub(crate) fn debugger_decode_capture(
        &self,
        context: &PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<ReplValue, String> {
        self.backend
            .as_deref()
            .ok_or_else(|| {
                "error[pure_native_backend_missing]: no active native execution backend".to_string()
            })?
            .decode_transition_value(context, boundary_type, value)
    }

    /// Resumes a stopped failure/debug continuation through a typed debugger restart.
    pub(crate) fn resume_debug_restart_for_actor(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
    ) -> Result<PureNativeExecution, String> {
        if suspension.owner_id() != context.owner_id() {
            return Err("error[pure_native_debug_restart_owner]: foreign suspension owner".into());
        }
        if !matches!(
            suspension.operation(),
            TvmTransitionOperation::Failure | TvmTransitionOperation::Debug
        ) {
            return Err(format!(
                "error[pure_native_debug_restart_operation]: cannot bypass {:?}",
                suspension.operation()
            ));
        }
        actors.resume_native_continuation(
            suspension.owner_id(),
            suspension.request_id(),
            suspension.continuation_id(),
        )?;
        self.finish_transition_resume(actors, context, suspension, Vec::new(), None, None)
    }
}
