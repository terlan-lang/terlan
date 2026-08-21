//! Runtime ownership glue for the portable Terlan service foundation.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use terlan_service_foundation::{
    FieldSet, LocalFormat, LocalSink, LogLevel, RequestContext, ServiceEvent, ServiceSink,
    SourceIdentity, TraceContext,
};

static SERVICE_EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static REQUEST_CONTEXT: RefCell<Option<RequestContext>> = const { RefCell::new(None) };
}

/// RAII scope that restores the prior context after a handler invocation.
pub(crate) struct RequestContextScope(Option<RequestContext>);

impl RequestContextScope {
    pub(crate) fn enter(context: RequestContext) -> Self {
        let prior = REQUEST_CONTEXT.with(|current| current.replace(Some(context)));
        Self(prior)
    }
}

impl Drop for RequestContextScope {
    fn drop(&mut self) {
        REQUEST_CONTEXT.with(|current| {
            current.replace(self.0.take());
        });
    }
}

pub(crate) struct RequestContextDescriptor<'a> {
    pub(crate) service: &'a str,
    pub(crate) route: &'a str,
    pub(crate) module: &'a str,
    pub(crate) function: &'a str,
    pub(crate) release_id: &'a str,
    pub(crate) source_file: &'a str,
    pub(crate) source_line: usize,
}

pub(crate) fn next_request_context(
    descriptor: RequestContextDescriptor<'_>,
    traceparent: Option<&str>,
) -> RequestContext {
    let request_id = SERVICE_EVENT_SEQUENCE
        .fetch_add(1, Ordering::Relaxed)
        .to_string();
    RequestContext {
        service: descriptor.service.to_owned(),
        connection_id: request_id.clone(),
        request_id,
        route_id: descriptor.route.to_owned(),
        handler_id: format!("{}.{}", descriptor.module, descriptor.function),
        release_id: descriptor.release_id.to_owned(),
        actor_id: None,
        source: SourceIdentity {
            module: descriptor.module.to_owned(),
            function: descriptor.function.to_owned(),
            file: descriptor.source_file.to_owned(),
            line: u32::try_from(descriptor.source_line).unwrap_or(u32::MAX),
        },
        trace: traceparent.and_then(|value| TraceContext::parse_traceparent(value).ok()),
    }
}

/// Emits application output through the local service sink. Sink failure is
/// intentionally contained and never changes the handler result.
pub(crate) fn emit_program_output(line: &str) {
    let context = REQUEST_CONTEXT.with(|current| current.borrow().clone());
    let sequence = SERVICE_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let event = ServiceEvent::Log {
        sequence,
        level: LogLevel::Info,
        message: line.to_owned(),
        fields: FieldSet::default(),
        context,
    };
    let sink = LocalSink::new(LocalFormat::Json, std::io::stderr());
    let _ = sink.emit(event);
}

#[cfg(test)]
#[path = "service_foundation_test.rs"]
mod tests;
