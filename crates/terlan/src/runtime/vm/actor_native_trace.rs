use super::super::meta_trace::VmMetaTraceCallToken;
use super::super::process::{VmProcessId, VmProcessSource};
use super::VmActorRuntime;

/// Diagnostic ownership retained from one native call until its terminal result.
#[derive(Debug)]
pub(crate) struct VmNativeTraceCall {
    source: Option<VmProcessSource>,
    meta_token: Option<VmMetaTraceCallToken>,
}

impl VmNativeTraceCall {
    /// Creates the empty trace state for an already-admitted service actor.
    pub(crate) const fn disabled() -> Self {
        Self {
            source: None,
            meta_token: None,
        }
    }
}

impl VmActorRuntime {
    /// Returns whether native calls need source metadata for an active trace.
    pub(crate) fn native_trace_enabled(&self) -> bool {
        !self.local_trace.is_empty() || !self.meta_trace.is_empty()
    }

    /// Publishes a native entry through the same exact-function diagnostic
    /// streams used by VM-owned source execution.
    pub(crate) fn begin_native_trace_call(
        &mut self,
        subject: VmProcessId,
        source: VmProcessSource,
    ) -> Result<VmNativeTraceCall, String> {
        self.begin_optional_native_trace_call(subject, Some(source))
    }

    /// Validates a native entry while allocating source metadata only when an
    /// active trace can observe it.
    pub(crate) fn begin_optional_native_trace_call(
        &mut self,
        subject: VmProcessId,
        source: Option<VmProcessSource>,
    ) -> Result<VmNativeTraceCall, String> {
        self.ensure_live_process(subject, "record native call for")?;
        let Some(source) = source else {
            return Ok(VmNativeTraceCall {
                source: None,
                meta_token: None,
            });
        };
        self.record_local_call(subject, source.clone(), 0)?;
        let meta_token = self.record_meta_call(subject, source.clone(), 0)?;
        Ok(VmNativeTraceCall {
            source: Some(source),
            meta_token,
        })
    }

    /// Publishes one successful native return with the actor's current caller.
    pub(crate) fn complete_native_trace_call(
        &mut self,
        subject: VmProcessId,
        call: VmNativeTraceCall,
    ) -> Result<(), String> {
        if let Some(source) = call.source {
            self.record_local_return(subject, source)?;
        }
        if let Some(token) = call.meta_token {
            self.record_meta_return(token, subject)?;
        }
        Ok(())
    }

    /// Publishes a native failure as an exception, never as a successful return.
    pub(crate) fn fail_native_trace_call(
        &mut self,
        subject: VmProcessId,
        call: VmNativeTraceCall,
        reason: impl Into<String>,
    ) -> Result<(), String> {
        match call.source {
            Some(source) => self
                .record_local_exception(subject, source, "native", reason)
                .map(|_| ()),
            None => Ok(()),
        }
    }
}
