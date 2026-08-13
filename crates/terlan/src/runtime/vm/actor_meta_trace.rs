use super::super::meta_trace::VmMetaTraceCallToken;
#[cfg(test)]
use super::super::meta_trace::{
    VmMetaTraceConfig, VmMetaTraceCursor, VmMetaTraceSnapshot, VmMetaTraceState,
};
use super::super::process::{VmProcessId, VmProcessLocation, VmProcessSource};
use super::VmActorRuntime;

impl VmActorRuntime {
    #[cfg(test)]
    pub(crate) fn enable_meta_trace(
        &mut self,
        source: VmProcessSource,
        observer: VmProcessId,
        config: VmMetaTraceConfig,
    ) -> Result<bool, String> {
        self.ensure_live_process(observer, "register meta trace observer")?;
        Ok(self.meta_trace.enable(source, observer, config))
    }

    #[cfg(test)]
    pub(crate) fn disable_meta_trace(&mut self, source: &VmProcessSource) -> bool {
        self.meta_trace.disable(source)
    }

    #[cfg(test)]
    pub(crate) fn meta_trace_state(&self, source: &VmProcessSource) -> VmMetaTraceState {
        self.meta_trace.state(source)
    }

    #[cfg(test)]
    pub(crate) fn meta_trace_cursor(&self) -> VmMetaTraceCursor {
        self.meta_trace.cursor()
    }

    #[cfg(test)]
    pub(crate) fn meta_trace_since(
        &self,
        cursor: VmMetaTraceCursor,
        observer: VmProcessId,
    ) -> Result<VmMetaTraceSnapshot, String> {
        self.meta_trace.since(cursor, observer)
    }

    pub(crate) fn record_meta_call(
        &mut self,
        subject: VmProcessId,
        source: VmProcessSource,
        instruction_offset: usize,
    ) -> Result<Option<VmMetaTraceCallToken>, String> {
        self.ensure_live_process(subject, "record meta call for")?;
        let Some(observer) = self.meta_trace.observer_for(&source) else {
            return Ok(None);
        };
        self.ensure_live_process(observer, "deliver meta call to")?;
        self.meta_trace.record_call(
            subject,
            VmProcessLocation {
                source,
                instruction_offset,
            },
        )
    }

    pub(crate) fn record_meta_return(
        &mut self,
        token: VmMetaTraceCallToken,
        subject: VmProcessId,
    ) -> Result<bool, String> {
        self.ensure_live_process(subject, "record meta return for")?;
        if token.subject != subject {
            return Err(format!(
                "VM meta trace return subject {} does not match call subject {}",
                subject.as_u64(),
                token.subject.as_u64()
            ));
        }
        let caller = self
            .processes
            .get(subject)
            .expect("live process was validated before meta return")
            .current_location()
            .clone();
        let observer_alive = self.is_alive(token.observer);
        self.meta_trace.record_return(token, caller, observer_alive)
    }
}
