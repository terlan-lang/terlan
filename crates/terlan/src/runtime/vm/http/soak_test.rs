use std::path::Path;

use super::soak::{run_release_http_soak, run_short_http_soak, VmHttpSoakReport};

#[test]
fn vm_http_short_soak_proves_resource_stability() {
    let report_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-http-soak-stability-report.json");
    let report = run_short_http_soak(&report_path).expect("short HTTP soak must pass");

    assert_eq!(report.profile, "short-deterministic");
    assert_eq!(report.cycles, 8);
    assert_eq!(report.canonical_requests, 40);
    assert_eq!(report.adversarial_replays, 1);
    assert_eq!(
        report.route_mix,
        vec!["static", "json", "add", "route-param", "stateful-counter"]
    );
    assert_eq!(report.accepted_requests, 71);
    assert_eq!(report.completed_requests, 58);
    assert_eq!(report.expected_failures, 14);
    assert_eq!(report.response_errors, 0);
    assert_eq!(report.disconnected_clients, 8);
    assert_eq!(report.backpressure_rejections, 1);
    assert_eq!(report.accept_queue_high_water, 16);
    assert_eq!(report.phase_resources.len(), 10);
    assert_phase_resources_release_everything(&report);
    assert!(report.parked_handlers >= 4);
    assert!(report.handler_wakeups >= 2);
    assert!(report.handler_wakeup_park_ratio_milli > 0);
    assert!(report.timer_cancellations >= 2);
    assert_eq!(report.timer_expirations, 1);
    assert!(report.response_memory_high_water_bytes > 0);
    assert!(
        report.response_memory_high_water_bytes
            <= report.stability_policy.max_response_memory_high_water_bytes
    );
    assert_eq!(report.response_memory_retained_bytes, 0);
    assert_eq!(report.peak_body_buffers, 1);
    assert_eq!(report.peak_body_bytes, 3);
    assert_eq!(report.peak_telemetry_spans, 1);
    assert_eq!(report.peak_route_contexts, 1);
    assert_eq!(report.final_heap_growth_bytes, 0);
    assert_eq!(report.final_resource_handle_growth, 0);
    assert_eq!(report.post_warmup_requests, 66);
    assert_eq!(report.post_warmup_error_rate_bps, 0);
    assert_eq!(report.terminals.len(), 16);
    assert_request_deadline_terminals(&report, 1);
    assert_pressure_terminals(&report, 1);
    assert_route_miss_terminals(&report, 1);
    assert!(report.steady_state_proven);
    assert!(report.leak_classifications.is_empty());
    assert_eq!(report.final_resources.process_live, 0);
    assert_eq!(report.final_resources.stream_open, 0);
    assert_eq!(report.final_resources.active_handlers, 0);
    assert_eq!(report.final_resources.active_timers, 0);
    assert!(report_path.is_file());
}

#[test]
fn vm_http_release_soak_replays_canonical_schedule_and_proves_stability() {
    let report_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-http-soak-release-stability-report.json");
    let report = run_release_http_soak(&report_path).expect("release HTTP soak must pass");

    assert_eq!(report.profile, "release-long");
    assert_eq!(report.cycles, 600);
    assert_eq!(report.canonical_requests, 3000);
    assert_eq!(report.adversarial_replays, 3);
    assert_eq!(report.accepted_requests, 3093);
    assert_eq!(report.completed_requests, 3054);
    assert_eq!(report.expected_failures, 42);
    assert_eq!(report.response_errors, 0);
    assert_eq!(report.disconnected_clients, 24);
    assert_eq!(report.backpressure_rejections, 3);
    assert_eq!(report.accept_queue_high_water, 16);
    assert_eq!(report.phase_resources.len(), 28);
    assert_phase_resources_release_everything(&report);
    assert!(report.parked_handlers >= 12);
    assert!(report.handler_wakeups >= 6);
    assert!(report.timer_cancellations >= 6);
    assert_eq!(report.timer_expirations, 3);
    assert_eq!(report.response_memory_retained_bytes, 0);
    assert_eq!(report.peak_body_buffers, 1);
    assert_eq!(report.peak_body_bytes, 3);
    assert_eq!(report.peak_telemetry_spans, 1);
    assert_eq!(report.peak_route_contexts, 1);
    assert_eq!(report.final_heap_growth_bytes, 0);
    assert_eq!(report.final_resource_handle_growth, 0);
    assert_eq!(report.post_warmup_requests, 3088);
    assert_eq!(report.post_warmup_error_rate_bps, 0);
    assert_eq!(report.terminals.len(), 48);
    assert_request_deadline_terminals(&report, 3);
    assert_pressure_terminals(&report, 3);
    assert_route_miss_terminals(&report, 3);
    assert!(report.steady_state_proven);
    assert!(report.leak_classifications.is_empty());
    assert_eq!(report.final_resources.process_live, 0);
    assert_eq!(report.final_resources.stream_open, 0);
    assert_eq!(report.final_resources.active_handlers, 0);
    assert_eq!(report.final_resources.active_timers, 0);
    assert_eq!(report.final_resources.active_body_buffers, 0);
    assert_eq!(report.final_resources.active_body_bytes, 0);
    assert_eq!(report.final_resources.active_telemetry_spans, 0);
    assert_eq!(report.final_resources.active_route_contexts, 0);
    assert!(report_path.is_file());
}

fn assert_phase_resources_release_everything(report: &VmHttpSoakReport) {
    assert!(report.phase_resources.iter().all(|phase| {
        let delta = &phase.retained_delta;
        delta.process_live == 0
            && delta.heap_bytes == 0
            && delta.native_boundary_handles == 0
            && delta.stream_open == 0
            && delta.queued_accepts == 0
            && delta.queued_messages == 0
            && delta.queued_bytes == 0
            && delta.waiting_readers == 0
            && delta.waiting_writers == 0
            && delta.active_handlers == 0
            && delta.active_timers == 0
            && delta.active_body_buffers == 0
            && delta.active_telemetry_spans == 0
            && delta.active_route_contexts == 0
    }));
}

fn assert_route_miss_terminals(report: &VmHttpSoakReport, replays: usize) {
    let terminals = report
        .terminals
        .iter()
        .filter(|terminal| terminal.phase == "route-miss")
        .collect::<Vec<_>>();
    assert_eq!(terminals.len(), replays);
    assert!(terminals.iter().all(|terminal| {
        terminal.outcome == "completed"
            && terminal
                .diagnostic
                .contains("status=404 shutdown_phase=adversarial_replay")
    }));
}

fn assert_request_deadline_terminals(report: &VmHttpSoakReport, replays: usize) {
    let terminals = report
        .terminals
        .iter()
        .filter(|terminal| terminal.phase == "request-deadline")
        .collect::<Vec<_>>();
    assert_eq!(terminals.len(), replays);
    assert!(terminals.iter().all(|terminal| {
        terminal.outcome == "timed_out" && terminal.diagnostic == "http_request_deadline_exceeded"
    }));
}

fn assert_pressure_terminals(report: &VmHttpSoakReport, replays: usize) {
    let disconnects = report
        .terminals
        .iter()
        .filter(|terminal| terminal.phase == "client-disconnect-storm")
        .collect::<Vec<_>>();
    assert_eq!(disconnects.len(), replays * 8);
    assert!(disconnects.iter().all(|terminal| {
        terminal.outcome == "disconnected"
            && terminal.diagnostic.contains("process=")
            && terminal.diagnostic.contains("terminal=peer_write_closed")
            && terminal
                .diagnostic
                .contains("shutdown_phase=adversarial_replay")
    }));

    let saturated = report
        .terminals
        .iter()
        .filter(|terminal| terminal.phase == "accept-queue-saturation")
        .collect::<Vec<_>>();
    assert_eq!(saturated.len(), replays);
    assert!(saturated.iter().all(|terminal| {
        terminal.outcome == "backpressured"
            && terminal.diagnostic.contains("limit=16 queued=16")
            && terminal
                .diagnostic
                .contains("terminal=backpressure_rejected")
            && terminal
                .diagnostic
                .contains("shutdown_phase=adversarial_replay")
    }));
}
