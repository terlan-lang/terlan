use super::soak::{
    stability::{stability_violations, VmHttpSoakStabilityInput, VmHttpSoakStabilityPolicy},
    VmHttpSoakResourceSnapshot,
};
use crate::runtime::vm::{
    http::request_resources::VmHttpRequestResourceLeak, process::VmProcessId,
};

#[test]
fn soak_stability_rejects_threshold_drift_with_stable_diagnostic_context() {
    let initial = empty_resources();
    let final_resources = empty_resources();
    let violations = stability_violations(VmHttpSoakStabilityInput {
        policy: VmHttpSoakStabilityPolicy {
            max_response_memory_high_water_bytes: 64,
            ..stability_policy()
        },
        initial: &initial,
        final_resources: &final_resources,
        response_memory_high_water_bytes: 65,
        response_memory_retained_bytes: 0,
        post_warmup_error_rate_bps: 0,
        last_request_id: 42,
        request_resource_leaks: &[],
    });

    assert_eq!(
        violations,
        ["leak_class=response_memory_high_water owner_process=none last_request_id=42 shutdown_phase=final observed=65 threshold=64"]
    );
}

#[test]
fn soak_stability_accepts_zero_growth_and_zero_retained_resources() {
    let initial = empty_resources();
    let final_resources = empty_resources();
    let violations = stability_violations(VmHttpSoakStabilityInput {
        policy: stability_policy(),
        initial: &initial,
        final_resources: &final_resources,
        response_memory_high_water_bytes: 512,
        response_memory_retained_bytes: 0,
        post_warmup_error_rate_bps: 0,
        last_request_id: 7,
        request_resource_leaks: &[],
    });

    assert!(violations.is_empty());
}

#[test]
fn soak_stability_classifies_every_transient_request_resource_leak() {
    let initial = empty_resources();
    let final_resources = empty_resources();
    let request_resource_leaks = [VmHttpRequestResourceLeak {
        owner: VmProcessId::from_raw_for_test(17),
        request_id: 23,
    }];
    let violations = stability_violations(VmHttpSoakStabilityInput {
        policy: stability_policy(),
        initial: &initial,
        final_resources: &final_resources,
        response_memory_high_water_bytes: 0,
        response_memory_retained_bytes: 0,
        post_warmup_error_rate_bps: 0,
        last_request_id: 23,
        request_resource_leaks: &request_resource_leaks,
    });

    assert_eq!(
        violations,
        [
            "leak_class=body_buffer owner_process=17 last_request_id=23 shutdown_phase=final observed=1 threshold=0",
            "leak_class=telemetry_span owner_process=17 last_request_id=23 shutdown_phase=final observed=1 threshold=0",
            "leak_class=route_context owner_process=17 last_request_id=23 shutdown_phase=final observed=1 threshold=0",
        ]
    );
}

fn stability_policy() -> VmHttpSoakStabilityPolicy {
    VmHttpSoakStabilityPolicy {
        max_response_memory_high_water_bytes: 4096,
        max_response_memory_retained_bytes: 0,
        max_final_heap_growth_bytes: 0,
        max_final_resource_handle_growth: 0,
        max_post_warmup_error_rate_bps: 0,
    }
}

fn empty_resources() -> VmHttpSoakResourceSnapshot {
    VmHttpSoakResourceSnapshot {
        process_total: 0,
        process_live: 0,
        process_exited: 0,
        mailbox_messages: 0,
        heap_bytes: 0,
        resource_handles: 0,
        native_boundary_handles: 0,
        listener_total: 0,
        listener_open: 0,
        stream_total: 0,
        stream_open: 0,
        queued_accepts: 0,
        queued_messages: 0,
        queued_bytes: 0,
        waiting_readers: 0,
        waiting_writers: 0,
        active_handlers: 0,
        active_timers: 0,
        active_body_buffers: 0,
        active_body_bytes: 0,
        active_telemetry_spans: 0,
        active_route_contexts: 0,
    }
}
