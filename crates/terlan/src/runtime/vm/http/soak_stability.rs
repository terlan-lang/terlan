use serde::Serialize;

use super::VmHttpSoakResourceSnapshot;
use crate::runtime::vm::http::request_resources::VmHttpRequestResourceLeak;

/// Configured release limits for VM HTTP soak stability.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmHttpSoakStabilityPolicy {
    pub(crate) max_response_memory_high_water_bytes: usize,
    pub(crate) max_response_memory_retained_bytes: usize,
    pub(crate) max_final_heap_growth_bytes: usize,
    pub(crate) max_final_resource_handle_growth: usize,
    pub(crate) max_post_warmup_error_rate_bps: usize,
}

/// Signed retained-resource delta for one soak phase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmHttpSoakResourceDelta {
    pub(crate) process_live: i64,
    pub(crate) heap_bytes: i64,
    pub(crate) native_boundary_handles: i64,
    pub(crate) listener_open: i64,
    pub(crate) stream_open: i64,
    pub(crate) queued_accepts: i64,
    pub(crate) queued_messages: i64,
    pub(crate) queued_bytes: i64,
    pub(crate) waiting_readers: i64,
    pub(crate) waiting_writers: i64,
    pub(crate) active_handlers: i64,
    pub(crate) active_timers: i64,
    pub(crate) active_body_buffers: i64,
    pub(crate) active_telemetry_spans: i64,
    pub(crate) active_route_contexts: i64,
}

/// Before/after ownership evidence for one deterministic soak phase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmHttpSoakPhaseResources {
    pub(crate) phase: &'static str,
    pub(crate) replay: Option<usize>,
    pub(crate) before: VmHttpSoakResourceSnapshot,
    pub(crate) after: VmHttpSoakResourceSnapshot,
    pub(crate) retained_delta: VmHttpSoakResourceDelta,
}

pub(super) fn phase_resources(
    phase: &'static str,
    replay: Option<usize>,
    before: VmHttpSoakResourceSnapshot,
    after: VmHttpSoakResourceSnapshot,
) -> VmHttpSoakPhaseResources {
    let retained_delta = resource_delta(&before, &after);
    VmHttpSoakPhaseResources {
        phase,
        replay,
        before,
        after,
        retained_delta,
    }
}

pub(in super::super) struct VmHttpSoakStabilityInput<'a> {
    pub(in super::super) policy: VmHttpSoakStabilityPolicy,
    pub(in super::super) initial: &'a VmHttpSoakResourceSnapshot,
    pub(in super::super) final_resources: &'a VmHttpSoakResourceSnapshot,
    pub(in super::super) response_memory_high_water_bytes: usize,
    pub(in super::super) response_memory_retained_bytes: usize,
    pub(in super::super) post_warmup_error_rate_bps: usize,
    pub(in super::super) last_request_id: u64,
    pub(in super::super) request_resource_leaks: &'a [VmHttpRequestResourceLeak],
}

pub(in super::super) fn stability_violations(input: VmHttpSoakStabilityInput<'_>) -> Vec<String> {
    let mut violations = final_resource_violations(
        input.final_resources,
        input.last_request_id,
        input.request_resource_leaks,
    );
    let heap_growth = growth(input.final_resources.heap_bytes, input.initial.heap_bytes);
    let handle_growth = growth(
        input.final_resources.native_boundary_handles,
        input.initial.native_boundary_handles,
    );
    let checks = [
        (
            "response_memory_high_water",
            input.response_memory_high_water_bytes,
            input.policy.max_response_memory_high_water_bytes,
        ),
        (
            "response_memory_retained",
            input.response_memory_retained_bytes,
            input.policy.max_response_memory_retained_bytes,
        ),
        (
            "heap_growth",
            heap_growth,
            input.policy.max_final_heap_growth_bytes,
        ),
        (
            "native_boundary_handle_growth",
            handle_growth,
            input.policy.max_final_resource_handle_growth,
        ),
        (
            "post_warmup_error_rate_bps",
            input.post_warmup_error_rate_bps,
            input.policy.max_post_warmup_error_rate_bps,
        ),
    ];
    violations.extend(checks.into_iter().filter_map(|(class, observed, limit)| {
        (observed > limit).then(|| {
            diagnostic(
                class,
                "none",
                input.last_request_id,
                "final",
                observed,
                limit,
            )
        })
    }));
    violations
}

fn final_resource_violations(
    resources: &VmHttpSoakResourceSnapshot,
    last_request_id: u64,
    request_resource_leaks: &[VmHttpRequestResourceLeak],
) -> Vec<String> {
    let candidates = [
        ("process", resources.process_live),
        ("mailbox", resources.mailbox_messages),
        ("heap", resources.heap_bytes),
        ("native_boundary_handle", resources.native_boundary_handles),
        ("listener", resources.listener_open),
        ("stream", resources.stream_open),
        ("accept_queue", resources.queued_accepts),
        ("stream_message", resources.queued_messages),
        ("stream_byte", resources.queued_bytes),
        ("reader_waiter", resources.waiting_readers),
        ("writer_waiter", resources.waiting_writers),
        ("handler", resources.active_handlers),
        ("timer", resources.active_timers),
    ];
    let mut violations = candidates
        .into_iter()
        .filter_map(|(class, observed)| {
            (observed != 0)
                .then(|| diagnostic(class, "none", last_request_id, "final", observed, 0))
        })
        .collect::<Vec<_>>();
    for leak in request_resource_leaks {
        for class in ["body_buffer", "telemetry_span", "route_context"] {
            violations.push(diagnostic(
                class,
                &leak.owner.as_u64().to_string(),
                leak.request_id,
                "final",
                1,
                0,
            ));
        }
    }
    violations
}

fn resource_delta(
    before: &VmHttpSoakResourceSnapshot,
    after: &VmHttpSoakResourceSnapshot,
) -> VmHttpSoakResourceDelta {
    VmHttpSoakResourceDelta {
        process_live: delta(before.process_live, after.process_live),
        heap_bytes: delta(before.heap_bytes, after.heap_bytes),
        native_boundary_handles: delta(
            before.native_boundary_handles,
            after.native_boundary_handles,
        ),
        listener_open: delta(before.listener_open, after.listener_open),
        stream_open: delta(before.stream_open, after.stream_open),
        queued_accepts: delta(before.queued_accepts, after.queued_accepts),
        queued_messages: delta(before.queued_messages, after.queued_messages),
        queued_bytes: delta(before.queued_bytes, after.queued_bytes),
        waiting_readers: delta(before.waiting_readers, after.waiting_readers),
        waiting_writers: delta(before.waiting_writers, after.waiting_writers),
        active_handlers: delta(before.active_handlers, after.active_handlers),
        active_timers: delta(before.active_timers, after.active_timers),
        active_body_buffers: delta(before.active_body_buffers, after.active_body_buffers),
        active_telemetry_spans: delta(before.active_telemetry_spans, after.active_telemetry_spans),
        active_route_contexts: delta(before.active_route_contexts, after.active_route_contexts),
    }
}

fn delta(before: usize, after: usize) -> i64 {
    let magnitude = i64::try_from(after.abs_diff(before)).unwrap_or(i64::MAX);
    if after >= before {
        magnitude
    } else {
        -magnitude
    }
}

fn growth(after: usize, before: usize) -> usize {
    after.saturating_sub(before)
}

fn diagnostic(
    class: &str,
    owner: &str,
    request_id: u64,
    shutdown_phase: &str,
    observed: usize,
    limit: usize,
) -> String {
    format!(
        "leak_class={class} owner_process={owner} last_request_id={request_id} shutdown_phase={shutdown_phase} observed={observed} threshold={limit}"
    )
}
