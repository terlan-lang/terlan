use super::{
    finish_http1_tcp_handler, request_resources::VmHttpRequestResourceTracker, VmHttpTcpHandler,
    VmHttpTcpServer,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessTable},
    tcp::{VmTcpRuntime, VmTcpStream},
};

#[cfg(test)]
pub(crate) use terlan_http_native::lifecycle::RequestOutcome as VmHttpRequestOutcome;
pub(crate) use terlan_http_native::lifecycle::ShutdownMode as VmHttpShutdownMode;
pub(crate) type VmHttpLifecycleEvent =
    terlan_http_native::lifecycle::LifecycleEvent<VmProcessId, VmTcpStream, VmExitReason>;
pub(crate) type VmHttpLifecycleHook =
    dyn terlan_http_native::lifecycle::LifecycleHook<VmProcessId, VmTcpStream, VmExitReason>;

pub(super) fn dispatch_http_handler(
    resources: &mut VmHttpRequestResourceTracker,
    lifecycle_hook: &mut Option<Box<VmHttpLifecycleHook>>,
    process: VmProcessId,
    request: ::http::Request<String>,
    handler: &mut impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<::http::Response<String>, String> {
    terlan_http_native::lifecycle::dispatch_handler(
        &mut resources.inner,
        lifecycle_hook,
        process,
        request,
        handler,
        super::request_resources::resource_error,
    )
}

impl VmHttpTcpServer {
    /// Installs one lifecycle hook for subsequent server transitions.
    #[cfg(test)]
    pub(crate) fn install_lifecycle_hook(
        &mut self,
        hook: impl terlan_http_native::lifecycle::LifecycleHook<VmProcessId, VmTcpStream, VmExitReason>
            + 'static,
    ) {
        self.lifecycle_hook = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(super) fn authorize_lifecycle(
        &mut self,
        event: &VmHttpLifecycleEvent,
    ) -> Result<(), String> {
        match self.lifecycle_hook.as_mut() {
            Some(hook) => hook.authorize(event),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    pub(super) fn observe_lifecycle(&mut self, event: &VmHttpLifecycleEvent) -> Result<(), String> {
        match self.lifecycle_hook.as_mut() {
            Some(hook) => hook.observe(event),
            None => Ok(()),
        }
    }

    /// Retains an admitted handler after lifecycle authorization.
    #[cfg(test)]
    pub(super) fn retain_handler(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        handler: VmHttpTcpHandler,
    ) -> Result<(), String> {
        let worker = VmHttpLifecycleEvent::WorkerStart {
            process: handler.process,
        };
        let channel = VmHttpLifecycleEvent::ChannelBind {
            process: handler.process,
            stream: handler.stream,
        };
        if let Err(error) = self
            .authorize_lifecycle(&worker)
            .and_then(|()| self.authorize_lifecycle(&channel))
        {
            finish_http1_tcp_handler(
                processes,
                tcp,
                &handler,
                VmExitReason::Error("VM HTTP lifecycle hook rejected handler".to_string()),
            )
            .map_err(|cleanup| format!("{error}; handler cleanup failed: {cleanup}"))?;
            return Err(error);
        }

        self.handlers.push(handler);
        if let Err(error) = self
            .observe_lifecycle(&worker)
            .and_then(|()| self.observe_lifecycle(&channel))
        {
            let handler = self.handlers.pop().expect("just-retained handler");
            finish_http1_tcp_handler(
                processes,
                tcp,
                &handler,
                VmExitReason::Error("VM HTTP lifecycle observation failed".to_string()),
            )
            .map_err(|cleanup| format!("{error}; handler cleanup failed: {cleanup}"))?;
            return Err(error);
        }
        Ok(())
    }

    /// Finishes a retained handler and publishes its non-vetoable unbind event.
    #[cfg(test)]
    pub(super) fn finish_handler(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        handler: &VmHttpTcpHandler,
        reason: VmExitReason,
    ) -> Result<Vec<String>, String> {
        let event = VmHttpLifecycleEvent::ChannelUnbind {
            process: handler.process,
            stream: handler.stream,
            reason: reason.clone(),
        };
        let cleanup = finish_http1_tcp_handler(processes, tcp, handler, reason)?;
        self.observe_lifecycle(&event)?;
        Ok(cleanup)
    }
}

#[cfg(test)]
mod lifecycle_hooks_test;
