use super::{
    finish_http1_tcp_handler, request_resources::VmHttpRequestResourceTracker, VmHttpTcpHandler,
    VmHttpTcpServer,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessTable},
    tcp::{VmTcpRuntime, VmTcpStream},
};

/// Typed result exposed after one HTTP request handler invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpRequestOutcome {
    Response { status: u16 },
    Error { message: String },
}

/// Shutdown transition exposed to HTTP lifecycle middleware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpShutdownMode {
    Drain,
    Immediate,
}

/// Typed VM HTTP lifecycle transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpLifecycleEvent {
    WorkerStart {
        process: VmProcessId,
    },
    RequestStart {
        process: VmProcessId,
        method: String,
        path: String,
    },
    RequestEnd {
        process: VmProcessId,
        method: String,
        path: String,
        outcome: VmHttpRequestOutcome,
    },
    ChannelBind {
        process: VmProcessId,
        stream: VmTcpStream,
    },
    ChannelUnbind {
        process: VmProcessId,
        stream: VmTcpStream,
        reason: VmExitReason,
    },
    ShutdownHandoff {
        mode: VmHttpShutdownMode,
        active_handlers: usize,
    },
}

/// Middleware-facing lifecycle hook for VM HTTP transitions.
///
/// `authorize` runs before policy-sensitive transitions and may reject them.
/// `observe` runs only after a transition succeeds. Cleanup transitions bypass
/// authorization so a hook cannot retain a process or stream accidentally.
pub(crate) trait VmHttpLifecycleHook {
    fn authorize(&mut self, _event: &VmHttpLifecycleEvent) -> Result<(), String> {
        Ok(())
    }

    fn observe(&mut self, _event: &VmHttpLifecycleEvent) -> Result<(), String> {
        Ok(())
    }
}

pub(super) fn dispatch_http_handler(
    resources: &mut VmHttpRequestResourceTracker,
    lifecycle_hook: &mut Option<Box<dyn VmHttpLifecycleHook>>,
    process: VmProcessId,
    request: ::http::Request<String>,
    handler: &mut impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
) -> Result<::http::Response<String>, String> {
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let start = VmHttpLifecycleEvent::RequestStart {
        process,
        method: method.clone(),
        path: path.clone(),
    };
    if let Some(hook) = lifecycle_hook.as_mut() {
        hook.authorize(&start)?;
    }
    let request_id = resources.begin(process, request.body().len())?;
    if let Some(hook) = lifecycle_hook.as_mut() {
        if let Err(error) = hook.observe(&start) {
            resources.finish(process, request_id)?;
            return Err(error);
        }
    }
    let result = handler(request);
    resources.finish(process, request_id)?;
    let outcome = match &result {
        Ok(response) => VmHttpRequestOutcome::Response {
            status: response.status().as_u16(),
        },
        Err(message) => VmHttpRequestOutcome::Error {
            message: message.clone(),
        },
    };
    let observed = match lifecycle_hook.as_mut() {
        Some(hook) => hook.observe(&VmHttpLifecycleEvent::RequestEnd {
            process,
            method,
            path,
            outcome,
        }),
        None => Ok(()),
    };
    match (result, observed) {
        (Ok(response), Ok(())) => Ok(response),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(handler), Err(hook)) => Err(format!(
            "{handler}; lifecycle observation failed after cleanup: {hook}"
        )),
    }
}

impl VmHttpTcpServer {
    /// Installs one lifecycle hook for subsequent server transitions.
    #[cfg(test)]
    pub(crate) fn install_lifecycle_hook(&mut self, hook: impl VmHttpLifecycleHook + 'static) {
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
