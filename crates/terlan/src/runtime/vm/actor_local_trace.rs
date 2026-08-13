#[cfg(test)]
use super::super::local_trace::{VmLocalTraceConfig, VmLocalTraceCursor, VmLocalTraceSnapshot};
use super::super::process::{VmProcessId, VmProcessLocation, VmProcessSource};
use super::VmActorRuntime;

impl VmActorRuntime {
    #[cfg(test)]
    pub(crate) fn enable_local_trace(
        &mut self,
        source: VmProcessSource,
        config: VmLocalTraceConfig,
    ) -> bool {
        self.local_trace.enable(source, config)
    }

    #[cfg(test)]
    pub(crate) fn disable_local_trace(&mut self, source: &VmProcessSource) -> bool {
        self.local_trace.disable(source)
    }

    #[cfg(test)]
    pub(crate) fn local_trace_enabled(&self, source: &VmProcessSource) -> bool {
        self.local_trace.is_enabled(source)
    }

    #[cfg(test)]
    pub(crate) fn local_trace_cursor(&self) -> VmLocalTraceCursor {
        self.local_trace.cursor()
    }

    #[cfg(test)]
    pub(crate) fn local_trace_since(
        &self,
        cursor: VmLocalTraceCursor,
    ) -> Result<VmLocalTraceSnapshot, String> {
        self.local_trace.since(cursor)
    }

    pub(crate) fn record_local_call(
        &mut self,
        pid: VmProcessId,
        source: VmProcessSource,
        instruction_offset: usize,
    ) -> Result<bool, String> {
        self.ensure_live_process(pid, "record local call for")?;
        self.local_trace.record_call(
            pid,
            VmProcessLocation {
                source,
                instruction_offset,
            },
        )
    }

    pub(crate) fn record_local_return(
        &mut self,
        pid: VmProcessId,
        source: VmProcessSource,
    ) -> Result<bool, String> {
        self.ensure_live_process(pid, "record local return for")?;
        let caller = self
            .processes
            .get(pid)
            .expect("live process was validated before local return")
            .current_location()
            .clone();
        self.local_trace.record_return(pid, source, caller)
    }

    pub(crate) fn record_local_exception(
        &mut self,
        pid: VmProcessId,
        source: VmProcessSource,
        class: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<bool, String> {
        self.ensure_live_process(pid, "record local exception for")?;
        let stack = self
            .processes
            .get(pid)
            .expect("live process was validated before local exception")
            .current_stacktrace();
        self.local_trace
            .record_exception(pid, source, class, reason, stack)
    }
}
