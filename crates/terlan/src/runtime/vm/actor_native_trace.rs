use super::super::meta_trace::VmMetaTraceCallToken;
use super::super::process::{VmProcessId, VmProcessSource};
use super::VmActorRuntime;

/// Diagnostic ownership retained from one native call until its terminal result.
#[derive(Debug)]
pub(crate) struct VmNativeTraceCall {
    source: VmProcessSource,
    meta_token: Option<VmMetaTraceCallToken>,
}

impl VmNativeTraceCall {
    pub(crate) fn source(&self) -> &VmProcessSource {
        &self.source
    }
}

impl VmActorRuntime {
    /// Publishes a native entry through the same exact-function diagnostic
    /// streams used by VM-owned source execution.
    pub(crate) fn begin_native_trace_call(
        &mut self,
        subject: VmProcessId,
        source: VmProcessSource,
    ) -> Result<VmNativeTraceCall, String> {
        self.ensure_live_process(subject, "record native call for")?;
        self.record_local_call(subject, source.clone(), 0)?;
        let meta_token = self.record_meta_call(subject, source.clone(), 0)?;
        Ok(VmNativeTraceCall { source, meta_token })
    }

    /// Publishes one successful native return with the actor's current caller.
    pub(crate) fn complete_native_trace_call(
        &mut self,
        subject: VmProcessId,
        call: VmNativeTraceCall,
    ) -> Result<(), String> {
        self.record_local_return(subject, call.source)?;
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
        self.record_local_exception(subject, call.source, "native", reason)
            .map(|_| ())
    }
}
