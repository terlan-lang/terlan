//! HTTP lifecycle policy; actor identities and transport handles stay host-owned.

use crate::request_resources::{RequestResourceError, RequestResourceTracker};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestOutcome {
    Response { status: u16 },
    Error { message: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownMode {
    Drain,
    Immediate,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LifecycleEvent<O, S, R> {
    WorkerStart {
        process: O,
    },
    RequestStart {
        process: O,
        method: String,
        path: String,
    },
    RequestEnd {
        process: O,
        method: String,
        path: String,
        outcome: RequestOutcome,
    },
    ChannelBind {
        process: O,
        stream: S,
    },
    ChannelUnbind {
        process: O,
        stream: S,
        reason: R,
    },
    ShutdownHandoff {
        mode: ShutdownMode,
        active_handlers: usize,
    },
}

/// Authorize before admission; observe after success. Cleanup cannot be vetoed.
pub trait LifecycleHook<O, S, R> {
    fn authorize(&mut self, _event: &LifecycleEvent<O, S, R>) -> Result<(), String> {
        Ok(())
    }

    fn observe(&mut self, _event: &LifecycleEvent<O, S, R>) -> Result<(), String> {
        Ok(())
    }
}

/// Executes a synchronous handler with ordered hooks and unconditional request
/// accounting cleanup. This does not schedule work or grant actor capabilities.
pub fn dispatch_handler<O: Copy + Ord, S, R, B>(
    resources: &mut RequestResourceTracker<O>,
    hook: &mut Option<Box<dyn LifecycleHook<O, S, R>>>,
    process: O,
    request: http::Request<String>,
    handler: &mut impl FnMut(http::Request<String>) -> Result<http::Response<B>, String>,
    resource_error: impl Fn(RequestResourceError<O>) -> String,
) -> Result<http::Response<B>, crate::ServiceError> {
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let start = LifecycleEvent::RequestStart {
        process,
        method: method.clone(),
        path: path.clone(),
    };
    if let Some(hook) = hook.as_mut() {
        hook.authorize(&start)?;
    }
    let request_id = resources
        .begin(process, request.body().len())
        .map_err(&resource_error)?;
    // The lease is private and cannot be finished by callbacks. Its drop also
    // covers unwinding from a host callback, without catching or masking panics.
    let lease = RequestLease {
        resources,
        owner: process,
        request_id,
    };
    if let Some(hook) = hook.as_mut() {
        hook.observe(&start)?;
    }
    let result = handler(request);
    drop(lease);
    let outcome = match &result {
        Ok(response) => RequestOutcome::Response {
            status: response.status().as_u16(),
        },
        Err(message) => RequestOutcome::Error {
            message: message.clone(),
        },
    };
    let observed = match hook.as_mut() {
        Some(hook) => hook.observe(&LifecycleEvent::RequestEnd {
            process,
            method,
            path,
            outcome,
        }),
        None => Ok(()),
    };
    match (result, observed) {
        (Ok(response), Ok(())) => Ok(response),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error.into()),
        (Err(handler), Err(hook)) => {
            Err(format!("{handler}; lifecycle observation failed after cleanup: {hook}").into())
        }
    }
}

struct RequestLease<'a, O: Copy + Ord> {
    resources: &'a mut RequestResourceTracker<O>,
    owner: O,
    request_id: u64,
}

impl<O: Copy + Ord> Drop for RequestLease<'_, O> {
    fn drop(&mut self) {
        // Only this lease can access the tracker during dispatch.
        let result = self.resources.finish(self.owner, self.request_id);
        debug_assert!(result.is_ok(), "private request lease must remain active");
    }
}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod tests;
