use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::{Condvar, Mutex};
use std::time::Instant;

use super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable};
use super::support_bundle::{
    VmSupportBundleReplayMetadata, VmSupportBundleReplayRecorder, VmSupportBundleReplayResource,
    VmSupportBundleReplayResourceKind,
};
use super::tcp::{VmTcpListener, VmTcpListenerInfo, VmTcpRuntime, VmTcpStream};
use super::tls::{VmTlsRuntime, VmTlsTcpPoll, VmTlsTcpServerStream, VmTlsTransportMode};
use deadline::VmHttpHandlerDeadlines;
use lifecycle::VmHttpLifecycleState;
use lifecycle_hooks::dispatch_http_handler;
pub(crate) use lifecycle_hooks::{VmHttpLifecycleEvent, VmHttpLifecycleHook, VmHttpShutdownMode};
pub(crate) use overload::{VmHttpOverloadConfig, VmHttpOverloadPolicy};
pub(crate) use request_read::read_http1_request;
use request_resources::{
    VmHttpRequestResourceLeak, VmHttpRequestResourceMetrics, VmHttpRequestResourceTracker,
};
use response_memory::VmHttpResponseMemory;
pub(crate) use response_wire::{
    write_http1_response, write_http1_stream_chunk, write_http1_stream_end,
    write_http1_stream_head,
};
pub(crate) use template_response::{render_http_template_response, VmHttpTemplateResponse};

/// Bounded VM HTTP scheduling queue.
///
/// Inputs:
/// - Accepted HTTP transport work items.
///
/// Output:
/// - Work items drained by a VM-owned HTTP handler worker, plus pressure
///   metrics for runtime inspection and benchmark reporting.
///
/// Transformation:
/// - Applies bounded backpressure at the accept-to-handler boundary without
///   depending on host async runtime state, OTP mailboxes, or a
///   benchmark-local queue contract.
pub(crate) struct VmHttpQueue<T> {
    capacity: usize,
    state: Mutex<VmHttpQueueState<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

/// Mutable state protected by the VM HTTP queue lock.
struct VmHttpQueueState<T> {
    items: VecDeque<T>,
    max_depth: usize,
    enqueue_count: usize,
    dequeue_count: usize,
    enqueue_wait_count: usize,
    enqueue_wait_total_ns: u128,
    dequeue_wait_count: usize,
    dequeue_wait_total_ns: u128,
    parked_producers: usize,
    parked_consumers: usize,
    max_parked_producers: usize,
    max_parked_consumers: usize,
    producer_wakeup_count: usize,
    consumer_wakeup_count: usize,
}

/// Snapshot of VM HTTP queue pressure.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmHttpQueueMetrics {
    pub(crate) current_depth: usize,
    pub(crate) max_depth: usize,
    pub(crate) enqueue_count: usize,
    pub(crate) dequeue_count: usize,
    pub(crate) enqueue_wait_count: usize,
    pub(crate) enqueue_wait_total_ns: u128,
    pub(crate) dequeue_wait_count: usize,
    pub(crate) dequeue_wait_total_ns: u128,
    pub(crate) parked_producers: usize,
    pub(crate) parked_consumers: usize,
    pub(crate) max_parked_producers: usize,
    pub(crate) max_parked_consumers: usize,
    pub(crate) producer_wakeup_count: usize,
    pub(crate) consumer_wakeup_count: usize,
}

/// Result of one VM HTTP/1 exchange over a VM TCP stream.
///
/// Inputs: parsed request metadata, response status, and emitted byte count.
/// Output: stable exchange telemetry for tests and runtime inspection.
/// Transformation: summarizes one protocol exchange without exposing stream
/// internals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpTcpExchange {
    pub(crate) request_method: String,
    pub(crate) request_path: String,
    pub(crate) response_status: u16,
    pub(crate) response_bytes: usize,
    pub(crate) close_connection: bool,
}

/// Result of one VM HTTP/1 exchange over caller-owned byte streams.
///
/// Inputs: parsed request metadata, response status, and emitted byte count.
/// Output: stable exchange telemetry for the in-memory VM HTTP baseline.
/// Transformation: summarizes one request/response cycle without depending on
/// VM TCP, TLS, host sockets, or framework callback state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpInMemoryExchange {
    pub(crate) request_method: String,
    pub(crate) request_path: String,
    pub(crate) response_status: u16,
    pub(crate) response_bytes: usize,
    pub(crate) close_connection: bool,
}

/// Buffered request state for pollable HTTP/1 exchanges over VM TCP.
///
/// Inputs: raw bytes received from one VM TCP stream. Output: retained partial
/// request bytes between scheduler polls. Transformation: lets the VM park a
/// handler until enough bytes arrive instead of treating an empty read as EOF.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmHttpTcpRequestBuffer {
    bytes: Vec<u8>,
}

/// One bounded request body chunk delivered to VM HTTP handler dispatch.
///
/// Inputs: UTF-8 request body bytes from a parsed HTTP request. Output:
/// stable chunk index, bytes, and final-chunk marker. Transformation: gives
/// handler dispatch an ordered stream view without copying full bodies into
/// support-bundle or source diagnostic paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpRequestBodyChunk {
    pub(crate) index: usize,
    pub(crate) bytes: Vec<u8>,
    pub(crate) is_final: bool,
}

/// VM-owned stream view over a parsed HTTP request body.
///
/// Inputs: typed HTTP request plus a max dispatch chunk size. Output:
/// deterministic chunks consumed by VM handler code. Transformation: decouples
/// body dispatch from handler execution while preserving body limits enforced
/// by the HTTP parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpRequestBodyStream {
    chunks: VecDeque<VmHttpRequestBodyChunk>,
    total_bytes: usize,
    max_chunk_bytes: usize,
}

impl VmHttpRequestBodyStream {
    pub(crate) fn next_chunk(&mut self) -> Option<VmHttpRequestBodyChunk> {
        self.chunks.pop_front()
    }

    pub(crate) fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub(crate) fn max_chunk_bytes(&self) -> usize {
        self.max_chunk_bytes
    }
}

/// Poll result for one VM HTTP/1 exchange over TCP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpTcpPoll {
    NeedRead,
    Complete(VmHttpTcpExchange),
}

/// Actor-facing poll result for one VM HTTP/1 exchange over TCP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpTcpActorPoll {
    Parked,
    Ready,
    Complete(VmHttpTcpExchange),
}

/// VM-owned HTTP handler process state for one accepted TCP stream.
///
/// Inputs: accepted VM TCP stream and spawned handler process id. Output:
/// retained handler state. Transformation: gives production HTTP a schedulable
/// VM-owned unit instead of a host-thread socket callback.
pub(crate) struct VmHttpTcpHandler {
    pub(crate) process: VmProcessId,
    pub(crate) stream: VmTcpStream,
    pub(crate) buffer: VmHttpTcpRequestBuffer,
    tls_stream: Option<VmTlsTcpServerStream>,
}

/// VM-owned HTTP/1 server state over a VM TCP listener.
///
/// Inputs: listener handle and source identity for spawned handlers. Output:
/// retained handler set and runtime counters. Transformation: provides a
/// schedulable production-shape HTTP loop without host socket ownership.
pub(crate) struct VmHttpTcpServer {
    listener: VmTcpListener,
    handler_source: VmProcessSource,
    overload: Option<VmHttpOverloadConfig>,
    handlers: Vec<VmHttpTcpHandler>,
    next_handler_index: usize,
    accepted_total: usize,
    rejected_total: usize,
    spilled_total: usize,
    completed_total: usize,
    response_memory: VmHttpResponseMemory,
    request_resources: VmHttpRequestResourceTracker,
    handler_timeout_ticks: Option<u64>,
    handler_deadlines: VmHttpHandlerDeadlines,
    last_completed_handlers: Vec<VmProcessId>,
    lifecycle: VmHttpLifecycleState,
    lifecycle_hook: Option<Box<dyn VmHttpLifecycleHook>>,
}

/// Snapshot of VM HTTP server scheduling and listener pressure.
///
/// Inputs: server counters plus listener state from `VmTcpRuntime`.
/// Output: inspection data suitable for diagnostics, benchmarks, and runtime
/// tooling. Transformation: keeps HTTP observability attached to the VM TCP
/// listener resource instead of exposing host socket state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpTcpServerInfo {
    pub(crate) listener: VmTcpListenerInfo,
    pub(crate) overload: Option<VmHttpOverloadConfig>,
    pub(crate) active_handlers: usize,
    pub(crate) next_handler_index: usize,
    pub(crate) accepted_total: usize,
    pub(crate) rejected_total: usize,
    pub(crate) spilled_total: usize,
    pub(crate) completed_total: usize,
}

/// Result of one VM HTTP server poll.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmHttpTcpServerPoll {
    pub(crate) accepted: usize,
    pub(crate) rejected: usize,
    pub(crate) spilled: usize,
    pub(crate) polled: usize,
    pub(crate) parked: usize,
    pub(crate) completed: usize,
    pub(crate) skipped_blocked: usize,
}

/// Deterministic replay seed for HTTP handler fairness support bundles.
///
/// Inputs:
/// - Queue pressure metrics, one HTTP server inspection snapshot, and one
///   scheduler poll report.
///
/// Output:
/// - Stable support-bundle seed that can replay the fairness-relevant shape of
///   a scheduler decision without retaining request bodies or socket bytes.
///
/// Transformation:
/// - Collapses volatile timing and socket state into bounded counters owned by
///   the VM HTTP runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpFairnessReplaySeed {
    pub(crate) seed_id: String,
    pub(crate) accepted: usize,
    pub(crate) polled: usize,
    pub(crate) parked: usize,
    pub(crate) skipped_blocked: usize,
    pub(crate) completed: usize,
    pub(crate) active_handlers: usize,
    pub(crate) next_handler_index: usize,
    pub(crate) queued_accepts: usize,
    pub(crate) max_queue_depth: usize,
}

/// Replayable support-bundle snapshot for one failed HTTP handler call.
///
/// Inputs:
/// - Stable process/source identity, parsed request metadata, failure text, and
///   VM-owned support-bundle replay metadata.
///
/// Output:
/// - Bounded failure evidence that can replay the handler-failure shape without
///   retaining raw request bodies, socket bytes, or host runtime state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpHandlerFailureReplayBundle {
    pub(crate) process: VmProcessId,
    pub(crate) handler_source: VmProcessSource,
    pub(crate) request_method: String,
    pub(crate) request_path: String,
    pub(crate) request_body_bytes: usize,
    pub(crate) failure: String,
    pub(crate) replay: VmSupportBundleReplayMetadata,
}

/// Source-linked diagnostic for one failed HTTP handler dispatch.
///
/// Inputs:
/// - Source-map file identity, handler source metadata, parsed request, and
///   failure message.
///
/// Output:
/// - Stable diagnostic fields that can be rendered by debugger, support-bundle,
///   and CLI tooling without inspecting host stack traces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpHandlerSourceDiagnostic {
    pub(crate) source_file: String,
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) request_method: String,
    pub(crate) request_path: String,
    pub(crate) message: String,
}

pub(crate) fn build_http_fairness_replay_seed(
    label: &str,
    poll: &VmHttpTcpServerPoll,
    server: &VmHttpTcpServerInfo,
    queue: &VmHttpQueueMetrics,
) -> Result<VmHttpFairnessReplaySeed, String> {
    if label.is_empty() {
        return Err("VM HTTP fairness replay seed label must not be empty".to_string());
    }
    let seed_id = format!(
        "{}:a{}:p{}:k{}:s{}:c{}:h{}:q{}",
        label,
        poll.accepted,
        poll.polled,
        poll.parked,
        poll.skipped_blocked,
        poll.completed,
        server.active_handlers,
        queue.max_depth
    );
    Ok(VmHttpFairnessReplaySeed {
        seed_id,
        accepted: poll.accepted,
        polled: poll.polled,
        parked: poll.parked,
        skipped_blocked: poll.skipped_blocked,
        completed: poll.completed,
        active_handlers: server.active_handlers,
        next_handler_index: server.next_handler_index,
        queued_accepts: server.listener.queued_accepts,
        max_queue_depth: queue.max_depth,
    })
}

/// Builds a source-map aware diagnostic for a failed HTTP handler.
///
/// Inputs:
/// - `source_file`: source-map file path selected by the VM artifact/debug
///   metadata.
/// - `handler_source`: VM process source identity for the handler entrypoint.
/// - `request`: parsed request metadata.
/// - `message`: stable failure text.
///
/// Output:
/// - A bounded diagnostic that preserves source-file/module/function/arity and
///   request method/path without retaining request bodies.
///
/// Transformation:
/// - Converts runtime handler failure context into source-linked diagnostic
///   evidence for CLI/debug/support-bundle consumers.
pub(crate) fn build_http_handler_source_diagnostic(
    source_file: impl Into<String>,
    handler_source: &VmProcessSource,
    request: &::http::Request<String>,
    message: impl Into<String>,
) -> Result<VmHttpHandlerSourceDiagnostic, String> {
    let source_file = source_file.into();
    if source_file.trim().is_empty() {
        return Err("VM HTTP handler diagnostic source file cannot be empty".to_string());
    }
    let message = message.into();
    if message.trim().is_empty() {
        return Err("VM HTTP handler diagnostic message cannot be empty".to_string());
    }
    Ok(VmHttpHandlerSourceDiagnostic {
        source_file,
        module: handler_source.module.clone(),
        function: handler_source.function.clone(),
        arity: handler_source.arity,
        request_method: request.method().as_str().to_string(),
        request_path: request.uri().path().to_string(),
        message,
    })
}

/// Captures replayable HTTP handler failure evidence for support bundles.
///
/// Inputs:
/// - Scheduler seed, handler process/source identity, stable resource handle,
///   parsed request, and failure message.
///
/// Output:
/// - A replay bundle with request method/path/body length, failure text, and a
///   finished support-bundle replay step.
///
/// Transformation:
/// - Converts volatile handler failure state into deterministic VM-owned replay
///   metadata without serializing request bodies or host socket details.
pub(crate) fn capture_http_handler_failure_support_bundle(
    scheduler_seed: u64,
    process: VmProcessId,
    handler_source: VmProcessSource,
    resource_handle: impl Into<String>,
    request: &::http::Request<String>,
    failure: impl Into<String>,
) -> Result<VmHttpHandlerFailureReplayBundle, String> {
    let failure = failure.into();
    if failure.trim().is_empty() {
        return Err("VM HTTP handler failure cannot be empty".to_string());
    }
    let request_method = request.method().as_str().to_string();
    let request_path = request.uri().path().to_string();
    let request_body_bytes = request.body().len();
    let resource_kind = VmSupportBundleReplayResourceKind::HttpHandler;
    let resource = VmSupportBundleReplayResource::new(resource_kind, resource_handle)?;
    let mut recorder = VmSupportBundleReplayRecorder::new(scheduler_seed);
    let outcome = format!("{request_method} {request_path}: {failure}");
    recorder
        .record_io_step(process, resource, "http.handler.failure", outcome)
        .expect("HTTP handler replay metadata uses non-empty operation and outcome");
    let replay = recorder.finish_bundle();
    Ok(VmHttpHandlerFailureReplayBundle {
        process,
        handler_source,
        request_method,
        request_path,
        request_body_bytes,
        failure,
        replay,
    })
}
